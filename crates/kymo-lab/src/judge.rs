//! Judging code a player wrote. The lab host runs it, one disposable container per run, and
//! answers with a verdict per test. What may run is fixed by the lab host's manifest: the image,
//! the file name, the compile and run commands. The platform sends only the source text, the
//! tests and the limits, so it can never pick an image or a command.

use std::path::{Path, PathBuf};
use std::time::Duration;

use kymo_instances::{Job, JobOutcome, JobRunner};
use serde::{Deserialize, Serialize};

pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_CASES: usize = 50;
pub const MAX_CASE_BYTES: usize = 256 * 1024;
pub const MIN_TIME_MS: u32 = 100;
pub const MAX_TIME_MS: u32 = 10_000;
pub const MIN_MEMORY_MB: u32 = 16;
pub const MAX_MEMORY_MB: u32 = 1024;
/// Wall clock allowed for the whole submission, compile included.
pub const TOTAL_BUDGET: Duration = Duration::from_secs(120);
/// Time the container needs to start, which a program's own time limit should not pay for.
const START_ALLOWANCE_MS: u64 = 500;
/// Characters of program output returned per test, for the author or the player to read.
const EXCERPT: usize = 2000;

/// What a manifest declares to make an image a language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgeBlock {
    /// What the source is saved as inside `/work`, for example `main.py` or `main.cpp`.
    pub file: String,
    /// Run once before the tests when the language compiles. Empty for an interpreted one.
    #[serde(default)]
    pub compile: Vec<String>,
    /// Run for every test, with the test input on standard input.
    pub run: Vec<String>,
    /// How players see it, for example `Python 3.12`.
    #[serde(default)]
    pub label: Option<String>,
    /// Seconds the compile step may take (default 30, at most 120).
    #[serde(default)]
    pub compile_timeout_secs: Option<u32>,
}

impl JudgeBlock {
    pub fn check(&self) -> Result<(), String> {
        let name_ok = !self.file.is_empty()
            && self.file.len() <= 64
            && !self.file.starts_with('.')
            && self
                .file
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !name_ok {
            return Err(
                "judge.file is a plain file name: letters, digits, . _ - and no leading dot".into(),
            );
        }
        let cmd_ok = |cmd: &[String]| {
            cmd.len() <= 16
                && cmd
                    .iter()
                    .all(|a| !a.is_empty() && a.len() <= 200 && !a.contains('\0'))
        };
        if self.run.is_empty() || !cmd_ok(&self.run) || !cmd_ok(&self.compile) {
            return Err("judge.run is a command of 1 to 16 arguments; judge.compile is empty or the same shape".into());
        }
        if self
            .compile_timeout_secs
            .is_some_and(|s| !(1..=120).contains(&s))
        {
            return Err("judge.compile_timeout_secs is 1 to 120".into());
        }
        Ok(())
    }
}

/// How the program's output is matched against the expected output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compare {
    /// Byte for byte.
    Exact,
    /// Line by line, ignoring spaces at the end of a line and empty lines at the end. The usual.
    #[default]
    Lines,
    /// Word by word, ignoring every kind of white space between them.
    Tokens,
}

impl Compare {
    pub fn matches(self, got: &str, expected: &str) -> bool {
        match self {
            Compare::Exact => got == expected,
            Compare::Lines => normalize_lines(got) == normalize_lines(expected),
            Compare::Tokens => got.split_whitespace().eq(expected.split_whitespace()),
        }
    }
}

fn normalize_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.trim_end_matches(['\r', ' ', '\t']))
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgeCase {
    pub input: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgeRequest {
    /// The manifest's artifact name; its `judge` block says how to run it.
    pub artifact: String,
    pub source: String,
    pub cases: Vec<JudgeCase>,
    pub time_limit_ms: u32,
    pub memory_mb: u32,
    #[serde(default)]
    pub compare: Compare,
    /// Stop at the first test that fails, to save the lab host's time.
    #[serde(default)]
    pub stop_on_fail: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseVerdict {
    Accepted,
    WrongAnswer,
    TimeLimit,
    MemoryLimit,
    RuntimeError,
    OutputLimit,
    /// Not run: an earlier test failed and the request asked to stop there.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseReport {
    pub verdict: CaseVerdict,
    pub ms: u64,
    pub exit_code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgeReport {
    /// What the compiler said, when the source did not compile. No test ran.
    pub compile_error: Option<String>,
    pub cases: Vec<CaseReport>,
}

impl JudgeReport {
    pub fn passed(&self) -> usize {
        self.cases
            .iter()
            .filter(|c| c.verdict == CaseVerdict::Accepted)
            .count()
    }
}

/// Why a request was refused before anything ran.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JudgeError {
    #[error("{0}")]
    Refused(String),
    #[error("the judge could not run: {0}")]
    Runner(String),
}

fn excerpt(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out: String = text.chars().take(EXCERPT).collect();
    if text.chars().count() > EXCERPT {
        out.push_str("\n[cut]");
    }
    out
}

pub fn check_request(request: &JudgeRequest) -> Result<(), JudgeError> {
    let refuse = |m: &str| Err(JudgeError::Refused(m.to_owned()));
    if request.source.is_empty() {
        return refuse("the source is empty");
    }
    if request.source.len() > MAX_SOURCE_BYTES {
        return refuse("the source is at most 64 KB");
    }
    if request.cases.is_empty() || request.cases.len() > MAX_CASES {
        return refuse("a judging has 1 to 50 tests");
    }
    if request
        .cases
        .iter()
        .any(|c| c.input.len() > MAX_CASE_BYTES || c.expected.len() > MAX_CASE_BYTES)
    {
        return refuse("a test input or output is at most 256 KB");
    }
    if !(MIN_TIME_MS..=MAX_TIME_MS).contains(&request.time_limit_ms) {
        return refuse("the time limit is 100 ms to 10 s");
    }
    if !(MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&request.memory_mb) {
        return refuse("the memory limit is 16 MB to 1 GB");
    }
    Ok(())
}

/// A private directory for one judging, removed when it is dropped, with whatever the program
/// left in it.
struct Workdir(PathBuf);

impl Workdir {
    fn create(root: &Path) -> std::io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(root)?;
        let name: String = (0..12)
            .map(|_| format!("{:02x}", rand::random::<u8>()))
            .collect();
        let dir = root.join(format!("job-{name}"));
        std::fs::create_dir(&dir)?;
        // The program runs as an unprivileged user and may need to write its build output here.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777))?;
        Ok(Self(dir))
    }
}

impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn verdict_of(
    outcome: &JobOutcome,
    time_limit_ms: u32,
    matched: impl FnOnce() -> bool,
) -> CaseVerdict {
    if outcome.oom {
        CaseVerdict::MemoryLimit
    } else if outcome.output_exceeded {
        CaseVerdict::OutputLimit
    } else if outcome.timed_out || outcome.wall_ms > u64::from(time_limit_ms) + START_ALLOWANCE_MS {
        CaseVerdict::TimeLimit
    } else if outcome.exit_code != Some(0) {
        CaseVerdict::RuntimeError
    } else if matched() {
        CaseVerdict::Accepted
    } else {
        CaseVerdict::WrongAnswer
    }
}

/// Compiles (when the language does) and runs every test. Never panics on hostile input: the
/// program's output is only ever read as bytes and cut.
pub async fn judge(
    runner: &dyn JobRunner,
    image: &str,
    block: &JudgeBlock,
    request: &JudgeRequest,
    root: &Path,
) -> Result<JudgeReport, JudgeError> {
    check_request(request)?;
    let work = Workdir::create(root).map_err(|e| JudgeError::Runner(e.to_string()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        let path = work.0.join(&block.file);
        std::fs::write(&path, request.source.as_bytes())
            .map_err(|e| JudgeError::Runner(e.to_string()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .map_err(|e| JudgeError::Runner(e.to_string()))?;
    }
    let started = std::time::Instant::now();
    let base = |cmd: &[String],
                writable: bool,
                stdin: Vec<u8>,
                timeout: Duration,
                memory_mb: u32,
                pids: u32| Job {
        image: image.to_owned(),
        cmd: cmd.to_vec(),
        dir: work.0.clone(),
        writable,
        stdin,
        timeout,
        memory_mb,
        cpu_millis: 1000,
        pids,
        output_limit: 1 << 20,
        file_size_mb: 16,
    };
    if !block.compile.is_empty() {
        let secs = u64::from(block.compile_timeout_secs.unwrap_or(30));
        let job = base(
            &block.compile,
            true,
            Vec::new(),
            Duration::from_secs(secs),
            request.memory_mb.max(512),
            128,
        );
        let out = runner
            .run_job(&job)
            .await
            .map_err(|e| JudgeError::Runner(e.to_string()))?;
        if out.timed_out || out.exit_code != Some(0) {
            let said = if out.timed_out {
                "the compiler took too long".to_owned()
            } else {
                let both = [out.stderr.as_slice(), out.stdout.as_slice()].concat();
                excerpt(&both)
            };
            return Ok(JudgeReport {
                compile_error: Some(said),
                cases: Vec::new(),
            });
        }
    }
    let mut cases = Vec::with_capacity(request.cases.len());
    let mut failed = false;
    // A program that hit a resource limit would hit it on the next test too, and each costs the
    // full time limit: the rest are not run.
    let mut heavy = false;
    for case in &request.cases {
        if (failed && request.stop_on_fail) || heavy || started.elapsed() > TOTAL_BUDGET {
            cases.push(CaseReport {
                verdict: CaseVerdict::Skipped,
                ms: 0,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            });
            continue;
        }
        let mut job = base(
            &block.run,
            false,
            case.input.clone().into_bytes(),
            Duration::from_millis(u64::from(request.time_limit_ms)),
            request.memory_mb,
            64,
        );
        // Room for the expected output twice over, and no more: a loop printing forever stops early.
        job.output_limit = (case.expected.len() * 2 + 4096).clamp(64 * 1024, 4 << 20);
        let out = runner
            .run_job(&job)
            .await
            .map_err(|e| JudgeError::Runner(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let verdict = verdict_of(&out, request.time_limit_ms, || {
            request.compare.matches(&stdout, &case.expected)
        });
        failed |= verdict != CaseVerdict::Accepted;
        heavy |= matches!(
            verdict,
            CaseVerdict::TimeLimit | CaseVerdict::MemoryLimit | CaseVerdict::OutputLimit
        );
        cases.push(CaseReport {
            verdict,
            ms: out
                .wall_ms
                .saturating_sub(START_ALLOWANCE_MS.min(out.wall_ms)),
            exit_code: out.exit_code,
            stdout: excerpt(&out.stdout),
            stderr: excerpt(&out.stderr),
        });
    }
    Ok(JudgeReport {
        compile_error: None,
        cases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kymo_instances::FakeJobRunner;

    fn block(compile: &[&str]) -> JudgeBlock {
        JudgeBlock {
            file: "main.py".into(),
            compile: compile.iter().map(|s| (*s).to_owned()).collect(),
            run: vec!["python3".into(), "main.py".into()],
            label: Some("Python".into()),
            compile_timeout_secs: None,
        }
    }

    fn request(cases: &[(&str, &str)]) -> JudgeRequest {
        JudgeRequest {
            artifact: "py".into(),
            source: "print(input())".into(),
            cases: cases
                .iter()
                .map(|(i, e)| JudgeCase {
                    input: (*i).into(),
                    expected: (*e).into(),
                })
                .collect(),
            time_limit_ms: 1000,
            memory_mb: 64,
            compare: Compare::Lines,
            stop_on_fail: false,
        }
    }

    fn ok(stdout: &str) -> JobOutcome {
        JobOutcome {
            exit_code: Some(0),
            stdout: stdout.as_bytes().to_vec(),
            wall_ms: 700,
            ..Default::default()
        }
    }

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!("kymo-judge-test-{}", rand::random::<u32>()))
    }

    #[test]
    fn output_is_compared_the_way_the_author_chose() {
        assert!(Compare::Lines.matches("1 2  \r\n3\n\n\n", "1 2\n3"));
        assert!(!Compare::Lines.matches("1 2\n3\n4", "1 2\n3"));
        assert!(
            !Compare::Lines.matches("1  2\n", "1 2\n"),
            "spaces inside a line count"
        );
        assert!(Compare::Tokens.matches("1   2\n\n3", "1 2 3"));
        assert!(!Compare::Tokens.matches("1 2", "1 2 3"));
        assert!(Compare::Exact.matches("a\n", "a\n"));
        assert!(!Compare::Exact.matches("a\n", "a"));
    }

    #[test]
    fn a_manifest_block_must_be_safe() {
        assert!(block(&[]).check().is_ok());
        for bad in ["../x.py", "a/b.py", ".hidden", "", "a b.py"] {
            let mut b = block(&[]);
            b.file = bad.into();
            assert!(b.check().is_err(), "{bad}");
        }
        let mut b = block(&[]);
        b.run.clear();
        assert!(b.check().is_err());
        let mut b = block(&[]);
        b.compile_timeout_secs = Some(500);
        assert!(b.check().is_err());
    }

    #[test]
    fn a_language_manifest_needs_no_port_but_an_instance_manifest_does() {
        let language: crate::manifest::Manifest = serde_json::from_value(serde_json::json!({
            "artifact": "python:3.12-alpine", "judge": { "file": "main.py", "run": ["python3", "main.py"] }
        }))
        .unwrap();
        assert!(language.check().is_ok(), "{:?}", language.check());
        let instance: crate::manifest::Manifest =
            serde_json::from_value(serde_json::json!({ "artifact": "web:1" })).unwrap();
        assert!(instance.check().is_err());
        let mixed: crate::manifest::Manifest = serde_json::from_value(serde_json::json!({
            "artifact": "x", "runtime": "vm", "file": "a.qcow2", "ports": [{"name": "ssh", "port": 22}],
            "judge": { "file": "main.py", "run": ["python3", "main.py"] }
        }))
        .unwrap();
        assert!(mixed.check().is_err(), "a machine cannot be a language");
    }

    #[test]
    fn the_request_is_bounded_before_anything_runs() {
        let fine = request(&[("1", "1")]);
        assert!(check_request(&fine).is_ok());
        let mut r = fine.clone();
        r.source = "x".repeat(MAX_SOURCE_BYTES + 1);
        assert!(check_request(&r).is_err());
        let mut r = fine.clone();
        r.cases.clear();
        assert!(check_request(&r).is_err());
        let mut r = fine.clone();
        r.time_limit_ms = 60_000;
        assert!(check_request(&r).is_err());
        let mut r = fine;
        r.memory_mb = 4096;
        assert!(check_request(&r).is_err());
    }

    #[tokio::test]
    async fn every_kind_of_failure_gets_its_own_verdict() {
        let runner = FakeJobRunner::new(|job| match String::from_utf8_lossy(&job.stdin).as_ref() {
            "good" => ok("42\n"),
            "wrong" => ok("41\n"),
            "slow" => JobOutcome {
                timed_out: true,
                wall_ms: 4000,
                ..Default::default()
            },
            "late" => JobOutcome {
                exit_code: Some(0),
                stdout: b"42\n".to_vec(),
                wall_ms: 2500,
                ..Default::default()
            },
            "oom" => JobOutcome {
                oom: true,
                exit_code: Some(137),
                ..Default::default()
            },
            "spam" => JobOutcome {
                output_exceeded: true,
                ..Default::default()
            },
            _ => JobOutcome {
                exit_code: Some(1),
                stderr: b"Traceback".to_vec(),
                wall_ms: 100,
                ..Default::default()
            },
        });
        use CaseVerdict::*;
        for (input, want) in [
            ("good", Accepted),
            ("wrong", WrongAnswer),
            ("slow", TimeLimit),
            ("late", TimeLimit),
            ("oom", MemoryLimit),
            ("spam", OutputLimit),
            ("crash", RuntimeError),
        ] {
            let report = judge(
                &runner,
                "img",
                &block(&[]),
                &request(&[(input, "42")]),
                &root(),
            )
            .await
            .unwrap();
            assert_eq!(report.cases[0].verdict, want, "{input}");
        }
        let report = judge(
            &runner,
            "img",
            &block(&[]),
            &request(&[("crash", "42")]),
            &root(),
        )
        .await
        .unwrap();
        assert_eq!(report.cases[0].stderr, "Traceback");
    }

    #[tokio::test]
    async fn a_resource_failure_ends_the_run_but_a_wrong_answer_does_not() {
        let runner = FakeJobRunner::new(|job| {
            if job.stdin == b"slow" {
                JobOutcome {
                    timed_out: true,
                    wall_ms: 4000,
                    ..Default::default()
                }
            } else {
                ok("42\n")
            }
        });
        use CaseVerdict::*;
        let r = request(&[("a", "42"), ("slow", "42"), ("b", "42"), ("c", "42")]);
        let report = judge(&runner, "img", &block(&[]), &r, &root())
            .await
            .unwrap();
        assert_eq!(
            report.cases.iter().map(|c| c.verdict).collect::<Vec<_>>(),
            [Accepted, TimeLimit, Skipped, Skipped]
        );
        assert_eq!(
            runner.seen.lock().unwrap().len(),
            2,
            "no container for the skipped tests"
        );
        let runner = FakeJobRunner::new(|job| {
            if job.stdin == b"bad" {
                ok("0")
            } else {
                ok("42\n")
            }
        });
        let report = judge(
            &runner,
            "img",
            &block(&[]),
            &request(&[("bad", "42"), ("b", "42")]),
            &root(),
        )
        .await
        .unwrap();
        assert_eq!(
            report.cases.iter().map(|c| c.verdict).collect::<Vec<_>>(),
            [WrongAnswer, Accepted]
        );
    }

    #[tokio::test]
    async fn the_source_is_saved_under_the_manifests_name_and_runs_read_only_with_the_test_on_stdin()
     {
        let runner = FakeJobRunner::new(|_| ok("1\n"));
        let r = request(&[("hello", "1")]);
        judge(&runner, "img", &block(&[]), &r, &root())
            .await
            .unwrap();
        let seen = runner.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let job = &seen[0];
        assert_eq!((job.image.as_str(), job.writable), ("img", false));
        assert_eq!(job.cmd, ["python3", "main.py"]);
        assert_eq!(job.stdin, b"hello");
        assert_eq!((job.memory_mb, job.pids, job.cpu_millis), (64, 64, 1000));
        assert!(
            job.dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("job-")
        );
    }

    #[tokio::test]
    async fn the_work_directory_is_removed_afterwards() {
        let runner = FakeJobRunner::new(|_| ok("1\n"));
        let root = root();
        judge(&runner, "img", &block(&[]), &request(&[("a", "1")]), &root)
            .await
            .unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn a_compile_step_runs_writable_first_and_a_failure_stops_everything() {
        let runner = FakeJobRunner::new(|job| {
            if job.writable {
                JobOutcome {
                    exit_code: Some(1),
                    stderr: b"main.cpp:1: error".to_vec(),
                    ..Default::default()
                }
            } else {
                ok("1\n")
            }
        });
        let report = judge(
            &runner,
            "img",
            &block(&["g++", "main.cpp"]),
            &request(&[("a", "1")]),
            &root(),
        )
        .await
        .unwrap();
        assert_eq!(report.compile_error.as_deref(), Some("main.cpp:1: error"));
        assert!(report.cases.is_empty());
        assert_eq!(runner.seen.lock().unwrap().len(), 1, "no test ran");
        // A compiler that works lets the tests run, read only.
        let runner = FakeJobRunner::new(|_| ok("1\n"));
        let report = judge(
            &runner,
            "img",
            &block(&["g++", "main.cpp"]),
            &request(&[("a", "1")]),
            &root(),
        )
        .await
        .unwrap();
        assert!(report.compile_error.is_none() && report.passed() == 1);
        let seen = runner.seen.lock().unwrap();
        assert_eq!((seen[0].writable, seen[1].writable), (true, false));
    }

    #[tokio::test]
    async fn stopping_at_the_first_failure_skips_the_rest() {
        let runner = FakeJobRunner::new(|job| {
            if job.stdin == b"bad" {
                ok("0")
            } else {
                ok("1\n")
            }
        });
        let mut r = request(&[("a", "1"), ("bad", "1"), ("b", "1"), ("c", "1")]);
        r.stop_on_fail = true;
        let report = judge(&runner, "img", &block(&[]), &r, &root())
            .await
            .unwrap();
        use CaseVerdict::*;
        assert_eq!(
            report.cases.iter().map(|c| c.verdict).collect::<Vec<_>>(),
            [Accepted, WrongAnswer, Skipped, Skipped]
        );
        assert_eq!(runner.seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn long_output_is_cut_and_binary_output_does_not_break_anything() {
        let runner = FakeJobRunner::new(|_| JobOutcome {
            exit_code: Some(0),
            stdout: [vec![b'a'; 5000], vec![0xff, 0xfe]].concat(),
            ..Default::default()
        });
        let report = judge(
            &runner,
            "img",
            &block(&[]),
            &request(&[("a", "x")]),
            &root(),
        )
        .await
        .unwrap();
        assert_eq!(report.cases[0].verdict, CaseVerdict::WrongAnswer);
        assert!(report.cases[0].stdout.ends_with("[cut]") && report.cases[0].stdout.len() < 2100);
    }
}
