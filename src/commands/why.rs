use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::{
    config::RuntimeContext,
    package::dependency_graph::{DependencyGraph, build},
};

pub fn execute(runtime: &RuntimeContext, package: &str) -> Result<()> {
    let target = package.trim();
    if target.is_empty() {
        bail!("package name cannot be empty");
    }

    let (graph, records) = build(runtime)?;
    if records.is_empty() {
        bail!("no packages installed");
    }
    if !records.iter().any(|record| record.package_name == target) {
        bail!("package '{target}' is not installed");
    }

    println!("{target}");
    let mut stack = HashSet::new();
    print_parents(&graph, target, "", &mut stack);
    Ok(())
}

fn print_parents(
    graph: &DependencyGraph,
    package: &str,
    prefix: &str,
    stack: &mut HashSet<String>,
) {
    let parents = graph.parents_of(package);
    if parents.is_empty() {
        println!("{prefix}`- installed directly");
        return;
    }

    for (idx, parent) in parents.iter().enumerate() {
        let is_last = idx == parents.len() - 1;
        let branch = if is_last { "`-" } else { "|-" };
        println!("{prefix}{branch} required by {parent}");
        if stack.contains(parent) {
            continue;
        }
        stack.insert(parent.clone());
        let child_prefix = if is_last {
            format!("{prefix}   ")
        } else {
            format!("{prefix}|  ")
        };
        print_parents(graph, parent, &child_prefix, stack);
        stack.remove(parent);
    }
}
