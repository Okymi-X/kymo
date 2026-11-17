//! One-off containers: run a program once with some input and get its output back. Used to judge
//! code a player wrote, so everything here assumes the program is hostile: no network, a read
//! only root, no capabilities, hard limits, and a container that never outlives the job.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::RuntimeError;

/// One run of one program.
#[derive(Debug, Clone)]
pub struct Job {
    pub image: String,
    /// The command, never passed through a shell.
    pub cmd: Vec<String>,
    /// Host directory mounted at `/work`, the working directory of the program.
    pub dir: PathBuf,
    /// Whether `/work` may be written (a compile step) or is read only (a run).
    pub writable: bool,
    pub stdin: Vec<u8>,
    /// Wall clock allowed before the container is killed.
    pub timeout: Duration,
    pub memory_mb: u32,
    pub cpu_millis: u32,
    pub pids: u32,
    /// Bytes of output kept per stream; more than this ends the job.
    pub output_limit: usize,
    /// Largest file the program may write, in megabytes.
    pub file_size_mb: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobOutcome {
    pub exit_code: Option<i64>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    /// Killed for using more memory than allowed.
    pub oom: bool,
    /// Wrote more than `output_limit`.
    pub output_exceeded: bool,
    pub wall_ms: u64,
}

#[async_trait]
pub trait JobRunner: Send + Sync {
    async fn run_job(&self, job: &Job) -> Result<JobOutcome, RuntimeError>;
}

type Script = Box<dyn Fn(&Job) -> JobOutcome + Send + Sync>;

/// A runner for tests: answers every job from a function and remembers what it was asked.
pub struct FakeJobRunner {
    script: Script,
    pub seen: Mutex<Vec<Job>>,
}

impl FakeJobRunner {
    pub fn new(script: impl Fn(&Job) -> JobOutcome + Send + Sync + 'static) -> Self {
        Self {
            script: Box::new(script),
            seen: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl JobRunner for FakeJobRunner {
    async fn run_job(&self, job: &Job) -> Result<JobOutcome, RuntimeError> {
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(job.clone());
        Ok((self.script)(job))
    }
}
