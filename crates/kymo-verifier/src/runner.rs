//! Runs the checks of a definition. Every function runs in its own process with its own
//! timeout and a clean working directory, and its exit status is the answer: 0 fixed, 1 the
//! weakness is still there, 2 broken. Anything else, a crash or a timeout is broken.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::definition::{Check, Language, Loaded};

const MAX_DETAIL: usize = 200;
const MAX_READ: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Fixed,
    Failed,
    Broken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub state: State,
    /// One line for the organizer. Never shown to players.
    pub detail: String,
    pub millis: u128,
}

/// Everything a script is told about the target, prepared once per run.
#[derive(Debug, Clone)]
pub struct Context {
    pub target_ip: String,
    pub target_ports: Vec<u16>,
    /// The directory holding `kymo_verify.sh` and `kymo_verify.py`.
    pub lib_dir: PathBuf,
    /// A private copy of the SSH key, readable by its owner only.
    key_file: Option<PathBuf>,
    _scratch: Option<std::sync::Arc<tempfile::TempDir>>,
}

impl Context {
    pub fn new(
        loaded: &Loaded,
        target_ip: &str,
        target_ports: Vec<u16>,
        lib_dir: &Path,
    ) -> std::io::Result<Context> {
        let ssh = loaded
            .definition
            .target
            .as_ref()
            .and_then(|t| t.ssh.as_ref());
        let mut key_file = None;
        let mut scratch = None;
        if let Some(key) = ssh.and_then(|s| s.key.as_ref()) {
            let dir = tempfile::tempdir()?;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
            let copy = dir.path().join("key");
            std::fs::copy(loaded.dir.join(key), &copy)?;
            std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o400))?;
            key_file = Some(copy);
            scratch = Some(std::sync::Arc::new(dir));
        }
        Ok(Context {
            target_ip: target_ip.to_owned(),
            target_ports,
            lib_dir: lib_dir.to_owned(),
            key_file,
            _scratch: scratch,
        })
    }
}

/// Runs every check in order. A check that fails or breaks does not stop the others. When the
/// run's own time is up, the checks that did not get a turn are broken.
pub async fn run_all(loaded: &Loaded, ctx: &Context) -> Vec<Outcome> {
    let deadline = Instant::now() + Duration::from_secs(loaded.definition.run_timeout_secs());
    let mut out = Vec::new();
    for check in &loaded.definition.checks {
        if Instant::now() >= deadline {
            out.push(Outcome {
                state: State::Broken,
                detail: "the run ran out of time".into(),
                millis: 0,
            });
            continue;
        }
        out.push(run_check(loaded, check, ctx).await);
    }
    out
}

/// One check: its health function first, when it has one, then the check itself.
pub async fn run_check(loaded: &Loaded, check: &Check, ctx: &Context) -> Outcome {
    let started = Instant::now();
    if let Some(health) = &check.health {
        let raw = call(loaded, check, health, ctx).await;
        if raw.code != Some(0) {
            return Outcome {
                state: State::Broken,
                detail: format!("health: {}", raw.describe()),
                millis: started.elapsed().as_millis(),
            };
        }
    }
    let raw = call(loaded, check, &check.function, ctx).await;
    Outcome {
        state: match raw.code {
            Some(0) => State::Fixed,
            Some(1) => State::Failed,
            _ => State::Broken,
        },
        detail: raw.describe(),
        millis: started.elapsed().as_millis(),
    }
}

struct Raw {
    code: Option<i32>,
    line: String,
    timed_out: bool,
}

impl Raw {
    fn describe(&self) -> String {
        if self.timed_out {
            return "timed out".into();
        }
        match (&self.code, self.line.is_empty()) {
            (None, true) => "ended without an exit status".into(),
            (_, false) => self.line.clone(),
            (Some(code), true) => format!("exit status {code}"),
        }
    }
}

const PYTHON_WRAPPER: &str = r#"
import importlib.util, sys
sys.path.insert(0, sys.argv[1])
import kymo_verify
try:
    spec = importlib.util.spec_from_file_location("kymo_check", sys.argv[2])
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    result = getattr(module, sys.argv[3])()
except SystemExit:
    raise
except BaseException as error:
    print("crashed: " + type(error).__name__)
    sys.exit(2)
sys.exit(kymo_verify.exit_code(result))
"#;

fn command(loaded: &Loaded, check: &Check, function: &str, ctx: &Context) -> Command {
    let file = loaded.dir.join(&check.file);
    let mut cmd = match check.language {
        Language::Bash => {
            let mut c = Command::new("bash");
            c.arg("-c")
                .arg(r#". "$1"; . "$2"; "$3""#)
                .arg("kymo-check")
                .arg(ctx.lib_dir.join("kymo_verify.sh"))
                .arg(&file)
                .arg(function);
            c
        }
        Language::Python => {
            let mut c = Command::new("python3");
            c.arg("-c")
                .arg(PYTHON_WRAPPER)
                .arg(&ctx.lib_dir)
                .arg(&file)
                .arg(function);
            c
        }
        Language::Exec => {
            let mut c = Command::new(&file);
            c.arg(function);
            c
        }
    };
    cmd.env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", "/tmp")
        .env("TARGET_IP", &ctx.target_ip)
        .env(
            "TARGET_PORTS",
            ctx.target_ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )
        .env("CHECK_ID", &check.id)
        .env("KYMO_VERIFY_LIB", ctx.lib_dir.join("kymo_verify.sh"))
        .env("SSH_HOST", &ctx.target_ip);
    if let Some(ssh) = loaded
        .definition
        .target
        .as_ref()
        .and_then(|t| t.ssh.as_ref())
    {
        cmd.env("SSH_PORT", ssh.port.to_string())
            .env("SSH_USER", &ssh.user)
            .env("SSH_SUDO", if ssh.sudo { "1" } else { "0" });
        if let Some(key) = &ctx.key_file {
            cmd.env("SSH_KEY_FILE", key);
        }
        if let Some(password) = &ssh.password {
            cmd.env("SSH_PASSWORD", password);
        }
    }
    cmd
}

async fn call(loaded: &Loaded, check: &Check, function: &str, ctx: &Context) -> Raw {
    let work = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            return Raw {
                code: None,
                line: format!("no working directory: {e}"),
                timed_out: false,
            };
        }
    };
    let mut cmd = command(loaded, check, function, ctx);
    cmd.current_dir(work.path())
        .env("WORKDIR", work.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        // Its own process group, so a timeout takes the whole tree down.
        .process_group(0);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Raw {
                code: None,
                line: format!("could not start: {e}"),
                timed_out: false,
            };
        }
    };
    let pid = child.id();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let wait = async {
        let mut out = String::new();
        let mut err = String::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.take(MAX_READ).read_to_string(&mut out).await;
        }
        if let Some(s) = stderr.as_mut() {
            let _ = s.take(MAX_READ).read_to_string(&mut err).await;
        }
        let status = child.wait().await;
        (out, err, status)
    };
    match tokio::time::timeout(Duration::from_secs(check.timeout_secs), wait).await {
        Ok((out, err, status)) => {
            let line = first_line(&out)
                .or_else(|| first_line(&err))
                .unwrap_or_default();
            Raw {
                code: status.ok().and_then(|s| s.code()),
                line,
                timed_out: false,
            }
        }
        Err(_) => {
            if let Some(pid) = pid.and_then(|p| i32::try_from(p).ok()) {
                // SAFETY: a plain kill(2) on the process group this function created.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
            Raw {
                code: None,
                line: String::new(),
                timed_out: true,
            }
        }
    }
}

/// The first non empty line, without control characters, cut to a sensible length.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| {
            l.chars()
                .filter(|c| !c.is_control())
                .take(MAX_DETAIL)
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::load;

    fn lib_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib")
    }

    /// A definition with bash checks, written to a temporary directory.
    fn bash_definition(
        checks: &[(&str, &str, Option<&str>, u64)],
        script: &str,
    ) -> (tempfile::TempDir, Loaded) {
        let dir = tempfile::tempdir().unwrap();
        let list: Vec<String> = checks
            .iter()
            .map(|(id, function, health, timeout)| {
                let health = health
                    .map(|h| format!(r#","health":"{h}""#))
                    .unwrap_or_default();
                format!(
                    r#"{{"id":"{id}","title":"Title {id}","language":"bash","file":"c.sh","function":"{function}","timeout_secs":{timeout}{health}}}"#
                )
            })
            .collect();
        std::fs::write(
            dir.path().join("verifier.json"),
            format!(r#"{{"version":1,"checks":[{}]}}"#, list.join(",")),
        )
        .unwrap();
        std::fs::write(dir.path().join("c.sh"), script).unwrap();
        let loaded = load(dir.path()).unwrap();
        (dir, loaded)
    }

    fn ctx(loaded: &Loaded) -> Context {
        Context::new(loaded, "10.77.0.42", vec![22, 80], &lib_dir()).unwrap()
    }

    #[tokio::test]
    async fn the_exit_status_of_the_function_is_the_answer() {
        let (_d, loaded) = bash_definition(
            &[
                ("a", "is_fixed", None, 5),
                ("b", "is_failed", None, 5),
                ("c", "is_broken", None, 5),
                ("d", "odd", None, 5),
            ],
            "is_fixed() { fixed; }\nis_failed() { vulnerable \"still open\"; }\nis_broken() { broken \"db is gone\"; }\nodd() { return 7; }\n",
        );
        let out = run_all(&loaded, &ctx(&loaded)).await;
        let states: Vec<State> = out.iter().map(|o| o.state).collect();
        assert_eq!(
            states,
            vec![State::Fixed, State::Failed, State::Broken, State::Broken]
        );
        assert_eq!(out[1].detail, "still open");
        assert_eq!(out[2].detail, "db is gone");
    }

    #[tokio::test]
    async fn the_health_function_gates_the_check() {
        let (_d, loaded) = bash_definition(
            &[
                ("a", "check", Some("healthy"), 5),
                ("b", "check", Some("sick"), 5),
            ],
            "healthy() { return 0; }\nsick() { echo no database; return 1; }\ncheck() { fixed; }\n",
        );
        let out = run_all(&loaded, &ctx(&loaded)).await;
        assert_eq!(out[0].state, State::Fixed);
        assert_eq!(out[1].state, State::Broken);
        assert!(out[1].detail.starts_with("health:"), "{}", out[1].detail);
    }

    #[tokio::test]
    async fn a_slow_check_times_out_as_broken_and_the_next_one_still_runs() {
        let (_d, loaded) = bash_definition(
            &[("slow", "slow", None, 1), ("quick", "quick", None, 5)],
            "slow() { sleep 30; fixed; }\nquick() { fixed; }\n",
        );
        let started = Instant::now();
        let out = run_all(&loaded, &ctx(&loaded)).await;
        assert_eq!(out[0].state, State::Broken);
        assert_eq!(out[0].detail, "timed out");
        assert_eq!(out[1].state, State::Fixed);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn scripts_get_the_target_and_a_clean_directory_and_nothing_else() {
        let (_d, loaded) = bash_definition(
            &[("env", "env_check", None, 5)],
            "env_check() {\n  [ \"$TARGET_IP\" = 10.77.0.42 ] || broken ip\n  [ \"$TARGET_PORTS\" = 22,80 ] || broken ports\n  [ \"$CHECK_ID\" = env ] || broken id\n  [ -z \"$(ls -A \"$WORKDIR\")\" ] || broken dirty\n  [ -z \"$KYMO_SECRET_FLAG\" ] || broken leak\n  fixed\n}\n",
        );
        let mut c = ctx(&loaded);
        c.target_ports = vec![22, 80];
        // SAFETY: tests in this crate do not read this variable.
        unsafe { std::env::set_var("KYMO_SECRET_FLAG", "KYMO{x}") };
        let out = run_all(&loaded, &c).await;
        assert_eq!(out[0].state, State::Fixed, "{}", out[0].detail);
    }

    #[tokio::test]
    async fn a_crash_or_a_missing_function_is_broken() {
        let (_d, loaded) = bash_definition(
            &[("crash", "crash", None, 5)],
            "crash() { kill -SEGV $$; }\n",
        );
        let out = run_all(&loaded, &ctx(&loaded)).await;
        assert_eq!(out[0].state, State::Broken);
        // A function the file does not define (the loader would have refused it) is broken too.
        let (_d2, mut other) = bash_definition(&[("a", "a", None, 5)], "a() { fixed; }\n");
        other.definition.checks[0].function = "not_there".into();
        let out = run_all(&other, &ctx(&other)).await;
        assert_eq!(out[0].state, State::Broken);
    }

    #[tokio::test]
    async fn the_run_has_its_own_time_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("verifier.json"),
            r#"{"version":1,"run_timeout_secs":1,"checks":[
                {"id":"a","title":"A","language":"bash","file":"c.sh","function":"slow","timeout_secs":5},
                {"id":"b","title":"B","language":"bash","file":"c.sh","function":"slow","timeout_secs":5}]}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("c.sh"), "slow() { sleep 2; fixed; }\n").unwrap();
        let loaded = load(dir.path()).unwrap();
        let out = run_all(&loaded, &ctx(&loaded)).await;
        assert_eq!(out[0].state, State::Fixed);
        assert_eq!(out[1].state, State::Broken);
        assert_eq!(out[1].detail, "the run ran out of time");
    }

    #[test]
    fn the_first_line_is_cut_and_cleaned() {
        assert_eq!(first_line("\n  hello\nworld"), Some("hello".into()));
        assert_eq!(first_line("a\u{7}b"), Some("ab".into()));
        assert_eq!(first_line(&"x".repeat(500)).unwrap().len(), MAX_DETAIL);
        assert_eq!(first_line(" \n "), None);
    }
}
