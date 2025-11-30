use crate::model::TaskSpec;
use anyhow::{anyhow, Result};
use tokio::time::{sleep, timeout, Duration};

/// Execute a single task according to its TaskSpec.
///
/// Returns:
/// - Ok(String)  => task succeeded, with an "output" message
/// - Err(anyhow::Error) => task failed
pub async fn execute_task(spec: &TaskSpec) -> Result<String> {
    // This inner async block contains the actual command logic.
    // We wrap it so we can optionally apply a timeout around the whole thing.
    let run = async {
        // Split the command by whitespace: "sleep 2" -> ["sleep", "2"]
        let parts: Vec<&str> = spec.command.split_whitespace().collect();

        match parts.as_slice() {
            // Pattern: ["sleep", n]
            ["sleep", n] => {
                let secs: u64 = n
                    .parse()
                    .map_err(|e| anyhow!("invalid sleep argument '{}': {}", n, e))?;
                sleep(Duration::from_secs(secs)).await;
                Ok(format!("slept for {} second(s)", secs))
            }

            // Pattern: ["echo", ..rest]
            ["echo", rest @ ..] => {
                let out = rest.join(" ");
                Ok(out)
            }

            // Anything else is unsupported in this simple demo executor
            other => Err(anyhow!("unsupported command: {:?}", other)),
        }
    };

    // Apply timeout if specified
    if let Some(secs) = spec.timeout_secs {
        let dur = Duration::from_secs(secs);
        match timeout(dur, run).await {
            Ok(result) => result, // result is Result<String>
            Err(_elapsed) => Err(anyhow!("task '{}' timed out after {}s", spec.id, secs)),
        }
    } else {
        // No timeout, just run it
        run.await
    }
}
