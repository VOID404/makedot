use std::{collections::HashMap, env, fs, io};

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
struct Var<'a>(&'a str, &'a str, &'a str);
#[derive(Debug)]
struct Task<'a> {
    phony: bool,
    name: &'a str,
    deps: Vec<&'a str>,
    body: Vec<&'a str>,
}

fn task_from_ast<'a>(
    phonies: &Vec<&str>,
    name: &'a str,
    deps: Vec<&'a str>,
    body: Vec<&'a str>,
) -> Task<'a> {
    Task {
        phony: phonies.contains(&name),
        name,
        deps,
        body,
    }
}

fn from_ast(terms: Vec<Term>) -> (HashMap<&str, Task>, HashMap<&str, Var>) {
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

    let mut tasks = HashMap::new();
    let mut vars = HashMap::new();

    for t in terms {
        match t {
            Term::Var(name, eq, val) => {
                vars.insert(name, Var(name, eq, val));
            }
            Term::Task {
                name,
                mut deps,
                body,
            } if name != ".PHONY" => {
                match tasks.get_mut(name) {
                    Some(t) => {
                        let t: &mut Task = t;
                        t.deps.append(&mut deps);
                    }
                    None => {
                        tasks.insert(name, task_from_ast(&phonies, name, deps, body));
                    }
                };
            }
            _ => (),
        }
    }

    (tasks, vars)
}

struct IDGen {
    uuid: i64,
}

impl IDGen {
    fn new() -> Self {
        IDGen { uuid: 0 }
    }
    fn next(&mut self) -> String {
        let id = self.uuid;
        self.uuid += 1;

        format!("id{}", self.uuid)
    }
}

struct SubGraph {
    name: String,
    nodes: Vec<Node>,
}

struct Node {
    id: String,
    label: String,
    children: Vec<String>,
}

impl Node {
    fn new(id_provider: &mut IDGen, label: String) -> Self {
        Self {
            id: id_provider.next(),
            label,
            children: vec![],
        }
    }
}

fn main() -> io::Result<()> {
    let re_var = Regex::new(r"\$\{\s*(\w+)\s*\}").unwrap();

    let args: Vec<String> = env::args().collect();
    assert!(args.len() == 2);
    let path: &str = &args[1];

    let file = fs::read_to_string(path)?;
    let terms = parse(&file);
    let (tasks, vars) = from_ast(terms);

    println!("strict graph {{");
    for (_, task) in tasks {
        for dep in task.deps {
            println!("\t\"{}\" -- \"{}\"", task.name, dep);
        }
        let external_deps = task.body.iter().filter(|l| l.contains("@make"));
        for dep in external_deps {
            let out = re_var.replace(dep, |caps: &Captures| match vars.get(&caps[1]) {
                Some(v) => v.2.to_string(),
                None => caps[0].to_string(),
            });
            println!("\t# {}", out);
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
