use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{self, Path, PathBuf},
    sync::OnceLock,
};

use regex::Regex;

use crate::{
    ast::{self, Parse as _},
    parser, Error,
};

type ID = String;
type Variables = HashMap<String, String>;

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: OnceLock<regex::Regex> = OnceLock::new();
        RE.get_or_init(|| regex::Regex::new($re).expect("Invalid regex!"))
    }};
}

pub struct IDGen(&'static str, usize);
impl IDGen {
    pub fn new(prefix: &'static str) -> Self {
        Self(prefix, 0)
    }

    pub fn next(&mut self) -> ID {
        let id = format!("{}{}", self.0, self.1);
        self.1 += 1;
        id
    }
}

#[derive(Debug)]
pub struct Task {
    pub phony: bool,
    pub name: String,
    pub dependencies: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug)]
pub struct Makefile {
    pub file: PathBuf,
    pub variables: Variables,
    pub tasks: HashMap<ID, Task>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarStr(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct External<T> {
    pub path: T,
    pub id: ID,
    pub tasks: Vec<String>,
}

impl<T> External<T> {
    fn map_path<U>(self, f: impl FnOnce(T) -> U) -> External<U> {
        External {
            path: f(self.path),
            id: self.id,
            tasks: self.tasks,
        }
    }
}

impl Makefile {
    pub fn get_id(&self, name: &str) -> Option<&ID> {
        self.tasks
            .iter()
            .find(|(_, t)| t.name == name)
            .map(|(id, _)| id)
    }
    pub fn walk_from(
        path: impl AsRef<Path>,
    ) -> Result<(Vec<Makefile>, HashSet<External<PathBuf>>), crate::Error> {
        let path = path.as_ref().to_path_buf();
        let mut out = Vec::new();
        let mut idgen = IDGen::new("task");
        let mut external: HashSet<External<PathBuf>> = HashSet::new();
        let mut paths = VecDeque::from([path]);

        while let Some(path) = paths.pop_front() {
            eprintln!("Parsing {}", path.display());
            let mut exts = HashSet::new();
            let data = std::fs::read_to_string(&path)?;
            let terms = parser::Makefile::parse(&data).map_err(|e| Error::from_nom(&data, e))?;
            let m = Makefile::from_terms(&mut idgen, &mut exts, path, terms);
            let exts = exts.iter().filter_map(|e| {
                let path = &e.path;
                let path = match m.resolve_makefile(path) {
                    Ok(p) => p,
                    Err(err) => {
                        eprintln!("Couldn't resolve makefile: {}, {}", path.0, err);
                        return None;
                    }
                };
                if !(paths.contains(&path) || out.iter().any(|m: &Makefile| m.file == path)) {
                    paths.push_back(path.clone());
                }

                Some(e.clone().map_path(|_| path))
            });
            external.extend(exts);
            out.push(m);
        }

        Ok((out, external))
    }

    pub fn resolve_vars(&self, str: &VarStr) -> String {
        let re_var = regex!(r"\$\{([^}]+)\}");
        let out = re_var
            .replace_all(&str.0, |v: &regex::Captures| {
                let key = v[1].to_string();
                self.variables.get(&key).unwrap_or(&str.0).to_string()
            })
            .into_owned();
        out
    }
    pub fn resolve_makefile(&self, path: &VarStr) -> Result<PathBuf, crate::Error> {
        let path = self
            .file
            .parent()
            .ok_or(Error::PathErr(format!(
                "Makefile path has no parent: {}",
                self.file.display()
            )))?
            .join(self.resolve_vars(path));
        let mut path = path.canonicalize().map_err(|err| {
            Error::PathErr(format!(
                "Couldn't canonicalize {},\n{}",
                path.display(),
                err
            ))
        })?;
        if path.is_dir() {
            path.push("Makefile");
        }

        Ok(path)
    }
    pub fn from_terms(
        id: &mut IDGen,
        external: &mut HashSet<External<VarStr>>,
        path: PathBuf,
        terms: Vec<ast::Term>,
    ) -> Self {
        let path = path.canonicalize().expect("Invalid makefile path");
        let mut out = Self {
            file: path,
            variables: Variables::new(),
            tasks: HashMap::new(),
        };

        let phonies = terms
            .iter()
            .filter_map(|t| match t {
                ast::Term::Task(t) if t.name == ".PHONY" => Some(t.dependencies.clone()),
                _ => None,
            })
            .flatten()
            .collect::<Vec<&str>>();

        for term in terms {
            match term {
                ast::Term::Task(t) => {
                    let id = id.next();
                    let dependencies = t.dependencies.into_iter().map(|v| v.to_string()).collect();
                    let commands = t
                        .commands
                        .into_iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<String>>();

                    external.extend(commands.iter().filter_map(|c| out.parse_make_line(c)).map(
                        |(path, tasks)| External {
                            path: VarStr(path),
                            id: id.clone(),
                            tasks,
                        },
                    ));

                    out.tasks.insert(
                        id,
                        Task {
                            phony: phonies.contains(&t.name),
                            name: t.name.to_string(),
                            dependencies,
                            commands,
                        },
                    );
                }
                ast::Term::Variable(v) => {
                    out.variables
                        .insert(v.name.to_string(), v.value.to_string());
                }
                ast::Term::Empty | ast::Term::Unimplemented(_) => (),
            }
        }

        out
    }

    fn parse_make_line(&self, line: &str) -> Option<(String, Vec<String>)> {
        let re_cmd = regex!(r"make (((\\\n)|([^\n#|&>]))+)\n?");
        let re_positionals = regex!(r"\s(\w[^=:\s/]+)(\s|$)");
        let re_path = regex!(r"(-C ?([^\s]+)|-f ?(((\\ )|[^\s])+))");
        let cmd = re_cmd.captures(line)?;
        let args = &cmd[1];
        let tasks = re_positionals
            .captures_iter(args)
            .map(|c| c[1].to_string())
            .collect::<Vec<String>>();
        let path = {
            let c = re_path.captures(args)?;
            let path = c.get(2).or_else(|| c.get(3))?;
            path.as_str().to_string()
        };
        eprintln!("Parsed {:?} {:?}", path, tasks);
        Some((path, tasks))
    }
}
