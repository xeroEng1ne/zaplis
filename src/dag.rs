use crate::cli::ZaplisArgs;
use crate::model::DagSpec;

use anyhow::{bail, Result};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Topo;
use std::collections::HashMap;
use std::fs;

/// Type alias for our DAG graph.
/// - Node weight: String (the task id)
/// - Edge weight: () (we don't store extra data on edges for now)
pub type Graph = DiGraph<String, ()>;

/// Build a petgraph `DiGraph` from a `DagSpec`.
///
/// Returns:
/// - the graph
/// - a map from task id -> NodeIndex (so we can look up nodes by id)
pub fn build_graph(spec: &DagSpec) -> Result<(Graph, HashMap<String, NodeIndex>)> {
    let mut graph = Graph::new();
    let mut index_by_id: HashMap<String, NodeIndex> = HashMap::new();

    // 1. Add all tasks as nodes
    for task in &spec.tasks {
        let idx = graph.add_node(task.id.clone());
        index_by_id.insert(task.id.clone(), idx);
    }

    // 2. Add edges: (from, to) means "from must run before to"
    for (from_id, to_id) in &spec.edges {
        let from_idx = index_by_id
            .get(from_id)
            .ok_or_else(|| anyhow::anyhow!("edge references unknown task id '{}'", from_id))?;

        let to_idx = index_by_id
            .get(to_id)
            .ok_or_else(|| anyhow::anyhow!("edge references unknown task id '{}'", to_id))?;

        graph.add_edge(*from_idx, *to_idx, ());
    }

    // 3. Validate that the graph is acyclic using a topological iterator.
    let mut topo = Topo::new(&graph);
    let mut visited = 0usize;
    while let Some(_node) = topo.next(&graph) {
        visited += 1;
    }

    if visited != graph.node_count() {
        bail!("DAG contains a cycle (not a valid DAG)");
    }

    Ok((graph, index_by_id))
}

/// Print a simple "plan" for the DAG:
/// - number of tasks
/// - number of edges
/// - topological order of tasks
pub fn print_plan(args: &ZaplisArgs) -> Result<()> {
    // 1. Read the DAG file from disk
    let text = fs::read_to_string(&args.file)?;
    let spec: DagSpec = serde_json::from_str(&text)?;

    // 2. Build and validate the graph
    let (graph, _index_by_id) = build_graph(&spec)?;

    println!("DAG file      : {}", args.file);
    println!("Task count    : {}", graph.node_count());
    println!("Dependency edges: {}", graph.edge_count());
    println!();

    // 3. Print a topological order (one valid execution order)
    println!("One possible topological order (respecting dependencies):");

    let mut topo = Topo::new(&graph);
    while let Some(node_idx) = topo.next(&graph) {
        let task_id = graph
            .node_weight(node_idx)
            .expect("node index should be valid");
        println!("  - {}", task_id);
    }

    Ok(())
}
