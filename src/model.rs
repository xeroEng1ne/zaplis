use serde::{Deserialize, Serialize};

/// Specification for a single task in the DAG.
///
/// This is how a task will look in your config file (JSON/YAML),
/// and also how we represent it in memory at runtime *before* it runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Unique identifier for this task within the DAG.
    /// Example: "compile", "test", "deploy-us-east"
    pub id: String,

    /// A simple "command" describing what to do.
    ///
    /// For now we'll support toy commands like:
    ///   - "sleep 2"
    ///   - "echo hello world"
    ///
    /// Later you could replace this with real shell commands or other actions.
    pub command: String,

    /// How many times to retry this task on failure.
    /// Default: 0 (no retries).
    #[serde(default)]
    pub retry: usize,

    /// Optional timeout in seconds.
    ///
    /// If None => no timeout.
    /// If Some(n) => fail the task if it runs longer than n seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Specification for the entire DAG.
///
/// This is what we expect to read from a file like `dag.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagSpec {
    /// List of task definitions.
    pub tasks: Vec<TaskSpec>,

    /// Edges of the DAG: (from, to) means:
    ///   - "from" must run and succeed before "to" can start.
    /// Example:
    ///   [("compile", "test"), ("test", "deploy")]
    pub edges: Vec<(String, String)>,
}

/// Runtime status of a task during execution.
///
/// This is *not* serialized to/from JSON for now; it's an in-memory state
/// used by the runner to track progress.
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Pending,
    Running,
    Success(String), // holds output / message
    Failed(String),  // holds error message
    Skipped,         // e.g., skipped because dependency failed
}

// {
//   "tasks": [
//     { "id": "a", "command": "sleep 1" },
//     { "id": "b", "command": "sleep 2" },
//     { "id": "c", "command": "echo c done" },
//     { "id": "d", "command": "echo all done", "retry": 1, "timeout_secs": 5 }
//   ],
//   "edges": [
//     ["a", "b"],
//     ["a", "c"],
//     ["b", "d"],
//     ["c", "d"]
//   ]
// }
