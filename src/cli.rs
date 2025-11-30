use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name="zaplis",
    version,
    about="Async DAG task runner in Rust",
    long_about=None
)]

pub struct ZaplisArgs{

    /// High-level command:
    /// - "run" : execute the DAG
    /// - "plan" : print/inspect the DAG
    #[arg(default_value="run")]
    pub command: String,

    /// Path to the DAG specification file (JSON for now).
    /// Example: --file dag.json
    #[arg(short, long, default_value="dag.json")]
    pub file: String,

    /// Maximum number of tasks to run in parallel.
    /// Example: -j 8  or  --max-concurrency 8
    #[arg(short='j',long="max-concurrency",default_value_t=4)]
    pub max_concurrency: usize,
}
