use crate::cli::ZaplisArgs;
use crate::dag;
use crate::executor::execute_task;
use crate::model::{DagSpec, TaskSpec, TaskStatus};

use anyhow::{Result, anyhow};
use serde_json;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tracing::{error, info, warn};

/// Run the DAG described in the file given by CLI args.
///
/// This function:
/// 1. Loads and parses the DAG spec from JSON.
/// 2. Validates it as a DAG (no cycles).
/// 3. Schedules tasks respecting dependencies.
/// 4. Runs tasks concurrently up to `max_concurrency`.
/// 5. Applies per-task retry policy.
/// 6. Prints final status of all tasks.
pub async fn run(args: ZaplisArgs) -> Result<()> {
    // ---------------------------------------------------------------------
    // 1. Load DAG spec from file
    // ---------------------------------------------------------------------
    let text = fs::read_to_string(&args.file)?;
    let spec: DagSpec = serde_json::from_str(&text)?;

    let total_tasks = spec.tasks.len();
    if total_tasks == 0 {
        info!("No tasks in DAG - nothing to do.");
        return Ok(());
    }

    // ---------------------------------------------------------------------
    // 2. Validate DAG structure (no cycles, all ids valid)
    // ---------------------------------------------------------------------
    dag::build_graph(&spec)?;
    info!(
        "Loaded DAG '{}' with {} tasks and {} edges",
        args.file,
        spec.tasks.len(),
        spec.edges.len()
    );

    // ---------------------------------------------------------------------
    // 3. Prepare scheduling data structures
    // ---------------------------------------------------------------------

    // Map: task id -> TaskSpec, for quick lookup
    let mut spec_map: HashMap<String, TaskSpec> = HashMap::new();
    for t in spec.tasks.into_iter() {
        spec_map.insert(t.id.clone(), t);
    }

    // Indegree: how many unmet dependencies each task has
    let mut indegree: HashMap<String, usize> = HashMap::new();
    // Dependents: for each task id, which tasks depend on it
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

    // Initialize indegree and dependents for all tasks
    for id in spec_map.keys() {
        indegree.insert(id.clone(), 0);
        dependents.insert(id.clone(), Vec::new());
    }

    // Fill indegree and dependents using edges
    for (from, to) in &spec.edges {
        // "to" has one more dependency
        *indegree
            .get_mut(to)
            .expect("edge references unknown task id (to)") += 1;

        // "from" has a dependent "to"
        dependents
            .get_mut(from)
            .expect("edge references unknown task id (from)")
            .push(to.clone());
    }

    // Ready queue: tasks with indegree == 0 (no unmet dependencies)
    let mut ready: VecDeque<String> = VecDeque::new();
    for (id, &deg) in indegree.iter() {
        if deg == 0 {
            ready.push_back(id.clone());
        }
    }

    // Track statuses for each task
    let mut statuses: HashMap<String, TaskStatus> = HashMap::new();
    for id in spec_map.keys() {
        statuses.insert(id.clone(), TaskStatus::Pending);
    }

    // ---------------------------------------------------------------------
    // 4. Concurrency control and communication channel
    // ---------------------------------------------------------------------

    // Semaphore to limit how many tasks run at the same time
    let semaphore = Arc::new(Semaphore::new(args.max_concurrency));

    // Channel to receive task completion events:
    // (task_id, Result<String>)
    let (tx, mut rx) = mpsc::channel::<(String, Result<String>)>(total_tasks);

    // For counting when we're done
    let mut completed_tasks: usize = 0;

    info!(
        "Starting DAG run with max_concurrency = {}",
        args.max_concurrency
    );

    // ---------------------------------------------------------------------
    // 5. Helper: function to spawn a task (NO status updates here)
    // ---------------------------------------------------------------------

    // This closure only:
    // - looks up TaskSpec
    // - runs it with retries
    // - sends result via channel
    //
    // It DOES NOT touch `statuses`, so we avoid borrow conflicts.
    let spec_map_ref = &spec_map;
    let mut spawn_task = move |task_id: String,
                               tx: mpsc::Sender<(String, Result<String>)>,
                               semaphore: Arc<Semaphore>| {
        // Lookup the TaskSpec and clone it to move into async task
        let spec = spec_map_ref
            .get(&task_id)
            .expect("task id must exist in spec_map")
            .clone();

        info!("Spawning task {}", task_id);

        tokio::spawn(async move {
            // Acquire semaphore permit (will wait if no capacity)
            let permit = semaphore.acquire_owned().await.unwrap();

            // Retry loop
            let max_attempts = spec.retry + 1;
            let mut last_result: Result<String> = Err(anyhow!("uninitialized"));

            for attempt in 1..=max_attempts {
                let res = execute_task(&spec).await;
                match res {
                    Ok(output) => {
                        last_result = Ok(output);
                        break;
                    }
                    Err(e) => {
                        if attempt == max_attempts {
                            last_result = Err(e);
                        } else {
                            warn!(
                                "task '{}' failed attempt {}/{}: {} - retrying",
                                spec.id, attempt, max_attempts, e
                            );
                        }
                    }
                }
            }

            // Release concurrency permit by dropping it
            drop(permit);

            // Send final result back to the runner
            if let Err(e) = tx.send((task_id.clone(), last_result)).await {
                error!("failed to send completion for task '{}': {}", task_id, e);
            }
        });
    };

    // ---------------------------------------------------------------------
    // 6. Main scheduling loop
    // ---------------------------------------------------------------------

    // Spawn all initially ready tasks
    while let Some(task_id) = ready.pop_front() {
        // ✅ update status OUTSIDE the closure
        statuses.insert(task_id.clone(), TaskStatus::Running);

        let tx_clone = tx.clone();
        let sem_clone = semaphore.clone();
        spawn_task(task_id, tx_clone, sem_clone);
    }

    // Process completions and spawn newly ready tasks
    while let Some((task_id, result)) = rx.recv().await {
        completed_tasks += 1;

        // Update status based on result
        match result {
            Ok(output) => {
                info!("Task '{}' succeeded: {}", task_id, output);
                statuses.insert(task_id.clone(), TaskStatus::Success(output));
            }
            Err(e) => {
                error!("Task '{}' failed: {}", task_id, e);
                statuses.insert(task_id.clone(), TaskStatus::Failed(e.to_string()));
            }
        }

        // Decrement indegree of dependents and push newly ready tasks
        if let Some(deps) = dependents.get(&task_id) {
            for dep_id in deps {
                if let Some(deg) = indegree.get_mut(dep_id) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        // All dependencies satisfied; ready to run
                        ready.push_back(dep_id.clone());
                    }
                }
            }
        }

        // Spawn any tasks that just became ready
        while let Some(next_id) = ready.pop_front() {
            // ✅ set status to Running before spawning
            statuses.insert(next_id.clone(), TaskStatus::Running);

            let tx_clone = tx.clone();
            let sem_clone = semaphore.clone();
            spawn_task(next_id, tx_clone, sem_clone);
        }

        // Stop when every task has completed
        if completed_tasks == total_tasks {
            info!("All tasks completed.");
            break;
        }
    }

    // ---------------------------------------------------------------------
    // 7. Print final summary
    // ---------------------------------------------------------------------
    println!("\nFinal task statuses:");
    for (id, status) in statuses.iter() {
        match status {
            TaskStatus::Pending => println!("  {} : PENDING", id),
            TaskStatus::Running => println!("  {} : RUNNING (should not remain)", id),
            TaskStatus::Success(out) => println!("  {} : SUCCESS -> {}", id, out),
            TaskStatus::Failed(err) => println!("  {} : FAILED -> {}", id, err),
            TaskStatus::Skipped => println!("  {} : SKIPPED", id),
        }
    }

    Ok(())
}
