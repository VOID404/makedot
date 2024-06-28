use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs, io,
    mem::MaybeUninit,
    path::{self, Path, PathBuf},
};

use ast::Term;
use pest::{iterators::Pair, Parser};
use pest_derive::Parser;
use regex::{Captures, Regex};

pub mod ast;

#[derive(Parser)]
#[grammar = "makefile.pest"]
pub struct MakefileParser;

fn parse(data: &str) -> Vec<Term> {
    let file = MakefileParser::parse(Rule::makefile, data)
        .unwrap()
        .next()
        .unwrap();

    let mut out = vec![];
    for term in file.into_inner() {
        match term.as_rule() {
            Rule::task => {
                let mut inner_rules = term.into_inner();
                let name = inner_rules.next().unwrap().as_str();

                let mut deps = vec![];
                let mut body = vec![];

                for t in inner_rules {
                    match t.as_rule() {
                        Rule::body => body = t.into_inner().map(|v| v.as_str()).collect(),
                        Rule::deps => deps = t.into_inner().map(|v| v.as_str()).collect(),
                        _ => (),
                    }
                }

                out.push(Term::Task { name, deps, body })
            }
            Rule::var => {
                let inner_rules = term.into_inner();
                let [name, eq, val]: [Pair<'_, Rule>; 3] =
                    inner_rules.collect::<Vec<_>>().try_into().unwrap();

                out.push(Term::Var(name.as_str(), eq.as_str(), val.as_str()))
            }
            Rule::EOI => (),
            _ => unreachable!(),
        }
    }

    return out;
}

#[derive(Debug)]
struct Var(String, String, String);
#[derive(Debug)]
struct Task {
    phony: bool,
    name: String,
    deps: Vec<String>,
    body: Vec<String>,
}

fn task_from_ast(phonies: &Vec<&str>, name: String, deps: Vec<String>, body: Vec<String>) -> Task {
    Task {
        phony: phonies.contains(&name.as_str()),
        name,
        deps,
        body,
    }
}

fn from_ast(terms: Vec<Term>) -> (HashMap<String, Task>, HashMap<String, Var>) {
    let empty = vec![];
    let phonies: Vec<&str> = {
        terms
            .iter()
            .flat_map(|t| match t {
                Term::Task {
                    name: ".PHONY",
                    deps,
                    ..
                } => deps,
                _ => &empty,
            })
            .map(|v| *v)
            .collect()
    };

    let mut tasks: HashMap<String, Task> = HashMap::new();
    let mut vars: HashMap<String, Var> = HashMap::new();

    for t in terms {
        match t {
            Term::Var(name, eq, val) => {
                vars.insert(
                    name.to_string(),
                    Var(name.to_string(), eq.to_string(), val.to_string()),
                );
            }
            Term::Task {
                name,
                mut deps,
                body,
            } if name != ".PHONY" => {
                match tasks.get_mut(name) {
                    Some(t) => {
                        let t: &mut Task = t;
                        t.deps.extend(deps.into_iter().map(|s| s.to_string()));
                    }
                    None => {
                        tasks.insert(
                            name.to_string(),
                            task_from_ast(
                                &phonies,
                                name.to_string(),
                                deps.into_iter().map(|s| s.to_string()).collect(),
                                body.into_iter().map(|s| s.to_string()).collect(),
                            ),
                        );
                    }
                };
            }
            _ => (),
        }
    }

    (tasks, vars)
}

fn parse_file(path: impl AsRef<Path>) -> io::Result<(HashMap<String, Task>, HashMap<String, Var>)> {
    let file = fs::read_to_string(path)?;
    let terms = parse(&file);
    let (tasks, vars) = from_ast(terms);
    Ok((tasks, vars))
}

type ID = String;

struct IDGen {
    prefix: String,
    uuid: i64,
}

impl IDGen {
    fn new() -> Self {
        Self::with("id")
    }
    fn with(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            uuid: 0,
        }
    }
    fn next(&mut self) -> ID {
        let id = self.uuid;
        self.uuid += 1;

        format!("{}{}", &self.prefix, id)
    }
}

fn walk_makefile(path: impl AsRef<Path>) -> io::Result<Vec<Makefile>> {
    let re_var = Regex::new(r"\$\{\s*(\w+)\s*\}").unwrap();
    let re_make = Regex::new(r"make\s+-C\s*([^\s]+) ([\w\-._]+)").unwrap();

    let mut queue = VecDeque::from([path::absolute(path.as_ref())?.canonicalize()?]);
    let mut makefile;
    let mut makefiles = vec![];

    let mut ids = IDGen::new();
    let mut id = || ids.next();

    while let Some(path) = queue.pop_back() {
        eprintln!("Parsing {}", path.to_string_lossy());
        makefile = Makefile {
            name: path.to_path_buf(),
            tasks: HashMap::new(),
            external: vec![],
        };
        let (tasks, vars) = parse_file(path)?;

        for (_, task) in tasks {
            let task_id = id();
            let external_deps = task.body.iter().filter(|l| l.contains("@make"));
            for dep in external_deps {
                let out = re_var.replace(dep, |caps: &Captures| match vars.get(&caps[1]) {
                    Some(v) => v.2.to_string(),
                    None => caps[0].to_string(),
                });
                match re_make.captures(&out) {
                    Some(c) => {
                        let mut path: PathBuf = makefile.name.clone();
                        path.pop();
                        path.push(&c[1]);
                        path.push("Makefile");
                        let path = path.canonicalize()?;
                        let this = task.name.clone();
                        let other = c[2].to_string();
                        // println!("Adding {:?}, {}, {}", path, this, other);
                        if !queue.contains(&path)
                            && !makefiles
                                .iter()
                                .find(|m: &&Makefile| m.name == path)
                                .is_some()
                        {
                            queue.push_front(path.clone());
                        }
                        // eprintln!(
                        //     "Adding external: {} {} from {}",
                        //     &task_id,
                        //     other,
                        //     path.to_string_lossy()
                        // );
                        makefile.external.push((path, task_id.clone(), other));
                    }
                    None => {}
                };
            }
            makefile.tasks.insert(task_id, task);
        }
        makefiles.push(makefile);
    }

    Ok(makefiles)
}

#[derive(Debug)]
struct Makefile {
    name: PathBuf,
    tasks: HashMap<String, Task>,
    external: Vec<(PathBuf, String, String)>,
}

impl Makefile {
    fn get_id(&self, name: &str) -> Option<&str> {
        self.tasks
            .iter()
            .find(|(_, task)| task.name == name)
            .map(|(id, _)| id.as_str())
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    assert!(args.len() == 2);
    let path: &str = &args[1];

    let mut ids = IDGen::with("cluster_");
    let makefiles = walk_makefile(path)?;
    eprintln!("Parsed {:#?} Makefiles", makefiles.len());

    println!("digraph G {{\n\tranksep=3");
    for makefile in &makefiles {
        println!(
            "\tsubgraph {} {{\n\t\tlabel=\"{}\"",
            ids.next(),
            makefile.name.to_string_lossy()
        );
        for (id, task) in &makefile.tasks {
            println!("\t\t{}[label=\"{}\"]", id, task.name);
            for dep in &task.deps {
                match makefile.get_id(&dep) {
                    Some(dep) => println!("\t\t{} -> {}", id, dep),
                    None => eprintln!(
                        "Bad task name: {} in {}",
                        dep,
                        makefile.name.to_string_lossy()
                    ),
                }
            }
        }

        println!("\t}}");
        for (source, task, dep) in &makefile.external {
            // eprintln!(
            //     "Finding {}: {} from {}",
            //     task,
            //     dep,
            //     source.to_string_lossy()
            // );
            match makefiles.iter().find(|m| &m.name == source) {
                Some(m) => match m.get_id(&dep) {
                    Some(dep) => {
                        // eprintln!("found: {} -> {}", task, dep);
                        println!("\t{} -> {}", task, dep)
                    }
                    None => eprintln!("Bad task name: {} in {}", dep, m.name.to_string_lossy()),
                },
                None => {}
            }
        }
    }

    println!("}}");

    Ok(())
}

#[test]
fn calculator1() {
    let parser = |v| MakefileParser::parse(Rule::var, v);
    let inputs = ["CLUSTER_NAME ?= asd", "REGISTRY_NAME = 123"];
    for input in inputs {
        let out = parser(input);
        println!("Here: {:#?}", out.unwrap())
    }
}
