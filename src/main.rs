use std::env::args;

use makefile::{IDGen, Makefile};
use nom::error::VerboseError;
use thiserror::Error;

mod ast;
mod makefile;
mod parser;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Parsing error:\n{0}")]
    ParseErr(String),

    #[error("{0}")]
    PathErr(String),
}

impl Error {
    pub fn from_nom(source: &str, err: nom::error::VerboseError<&str>) -> Self {
        let str = nom::error::convert_error(source, err);
        Self::ParseErr(str)
    }
}

fn main() {
    // TODO: clap arg parser
    let path = args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: {} <makefile>", args().next().unwrap());
        std::process::exit(1);
    });

    eprintln!("Starting at {}", path);

    let (makefiles, externals) = match Makefile::walk_from(path) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("Error walking makefile:\n{}", err);
            std::process::exit(1);
        }
    };

    let mut id = IDGen::new("cluster_");
    println!("digraph G {{\n\tranksep=3");
    for makefile in makefiles.iter() {
        println!(
            "\tsubgraph {} {{\n\t\tlabel=\"{}\"",
            id.next(),
            makefile.file.display()
        );

        for (id, task) in &makefile.tasks {
            println!("\t\t{}[label=\"{}\"]", id, task.name);
            for dep in task.dependencies.iter() {
                match makefile.get_id(dep) {
                    Some(dep_id) => println!("\t\t{} -> {}", id, dep_id),
                    None => eprintln!("Bad dependency: {}", dep),
                }
            }
        }
        println!("\t}}");
    }

    for external in externals.iter() {
        let m = match makefiles.iter().find(|m| m.file == external.path) {
            Some(v) => v,
            None => {
                eprintln!("External makefile not found: {:?}", external.path);
                continue;
            }
        };

        for task in external.tasks.iter() {
            match m.get_id(task) {
                Some(task_id) => println!("\t{} -> {}", external.id, task_id),
                None => eprintln!("External task not found: {}", task),
            }
        }
    }
    println!("}}");
}
