use std::{
    collections::{HashMap, HashSet},
    env::args,
    path::PathBuf,
};

use ast::{Parse as _, Term};
use makefile::{IDGen, Makefile};
use nom::error::convert_error;

mod ast;
mod makefile;
mod parser;

fn main() {
    let path = args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: {} <makefile>", args().next().unwrap());
        std::process::exit(1);
    });

    let (makefiles, externals) = Makefile::walk_from(path);

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
