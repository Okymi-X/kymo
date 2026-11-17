//! The judge against a real Docker daemon, with hostile programs. Skipped (with a note) when
//! Docker or the python:3.12-alpine image is missing, so the suite still runs anywhere.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use kymo_instances::docker::DockerRuntime;
use kymo_lab::judge::{
    CaseVerdict, Compare, JudgeBlock, JudgeCase, JudgeReport, JudgeRequest, judge,
};

const IMAGE: &str = "python:3.12-alpine";

fn available() -> bool {
    std::process::Command::new("docker")
        .args(["image", "inspect", IMAGE])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn block() -> JudgeBlock {
    JudgeBlock {
        file: "main.py".into(),
        compile: vec![],
        run: vec!["python3".into(), "main.py".into()],
        label: None,
        compile_timeout_secs: None,
    }
}

async fn run(source: &str, input: &str, expected: &str, ms: u32, mb: u32) -> JudgeReport {
    let runtime = DockerRuntime::connect_scoped("127.0.0.1", "judgetest").unwrap();
    let root: PathBuf = std::env::temp_dir().join("kymo-judge-docker-test");
    let request = JudgeRequest {
        artifact: IMAGE.into(),
        source: source.into(),
        cases: vec![JudgeCase {
            input: input.into(),
            expected: expected.into(),
        }],
        time_limit_ms: ms,
        memory_mb: mb,
        compare: Compare::Lines,
        stop_on_fail: false,
    };
    judge(&runtime, IMAGE, &block(), &request, &root)
        .await
        .unwrap()
}

macro_rules! need_docker {
    () => {
        if !available() {
            eprintln!("skipped: Docker or {IMAGE} is not available");
            return;
        }
    };
}

#[tokio::test]
async fn a_correct_and_a_wrong_program_are_told_apart() {
    need_docker!();
    let ok = run(
        "print(sum(map(int, input().split())))",
        "2 3 4\n",
        "9\n",
        2000,
        64,
    )
    .await;
    assert_eq!(ok.cases[0].verdict, CaseVerdict::Accepted, "{ok:?}");
    let wrong = run("print(1)", "", "2\n", 2000, 64).await;
    assert_eq!(wrong.cases[0].verdict, CaseVerdict::WrongAnswer);
    let crash = run("raise SystemExit(3)", "", "", 2000, 64).await;
    assert_eq!(crash.cases[0].verdict, CaseVerdict::RuntimeError);
    assert_eq!(crash.cases[0].exit_code, Some(3));
}

#[tokio::test]
async fn an_endless_loop_is_stopped_at_the_time_limit() {
    need_docker!();
    let started = Instant::now();
    let r = run("while True: pass", "", "", 1000, 64).await;
    assert_eq!(r.cases[0].verdict, CaseVerdict::TimeLimit, "{r:?}");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_memory_hog_is_killed() {
    need_docker!();
    let r = run(
        "x = bytearray(900 * 1024 * 1024)\nprint(len(x))",
        "",
        "",
        5000,
        64,
    )
    .await;
    assert_ne!(r.cases[0].verdict, CaseVerdict::Accepted);
    assert!(
        matches!(
            r.cases[0].verdict,
            CaseVerdict::MemoryLimit | CaseVerdict::RuntimeError
        ),
        "{r:?}"
    );
}

#[tokio::test]
async fn a_fork_bomb_does_not_get_past_the_process_limit() {
    need_docker!();
    let started = Instant::now();
    let r = run("import os\nwhile True:\n    os.fork()", "", "", 2000, 64).await;
    assert_ne!(r.cases[0].verdict, CaseVerdict::Accepted, "{r:?}");
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn an_output_flood_is_cut_short() {
    need_docker!();
    let r = run("while True:\n    print('a' * 1000)", "", "x", 3000, 64).await;
    assert!(
        matches!(
            r.cases[0].verdict,
            CaseVerdict::OutputLimit | CaseVerdict::TimeLimit
        ),
        "{r:?}"
    );
    assert!(r.cases[0].stdout.len() < 2100);
}

#[tokio::test]
async fn the_program_has_no_network_and_cannot_write_outside_its_scratch_space() {
    need_docker!();
    let net = run("import socket\ns = socket.socket()\ns.settimeout(2)\ns.connect(('1.1.1.1', 80))\nprint('reached')", "", "reached", 5000, 64).await;
    assert_ne!(net.cases[0].verdict, CaseVerdict::Accepted, "{net:?}");
    assert!(!net.cases[0].stdout.contains("reached"));
    let write = run(
        "open('/work/evil', 'w').write('x')\nprint('wrote')",
        "",
        "wrote",
        3000,
        64,
    )
    .await;
    assert_ne!(write.cases[0].verdict, CaseVerdict::Accepted, "{write:?}");
    let root_fs = run(
        "open('/etc/evil', 'w').write('x')\nprint('wrote')",
        "",
        "wrote",
        3000,
        64,
    )
    .await;
    assert_ne!(
        root_fs.cases[0].verdict,
        CaseVerdict::Accepted,
        "{root_fs:?}"
    );
    // It runs as nobody, not root.
    let who = run("import os\nprint(os.getuid())", "", "65534", 3000, 64).await;
    assert_eq!(who.cases[0].verdict, CaseVerdict::Accepted, "{who:?}");
}

#[tokio::test]
async fn no_container_is_left_behind() {
    need_docker!();
    run("print(1)", "", "1", 2000, 64).await;
    run("while True: pass", "", "", 500, 64).await;
    // The other tests share the label and may still be running: wait for it to drain.
    let mut left = String::new();
    for _ in 0..40 {
        let out = std::process::Command::new("docker")
            .args(["ps", "-aq", "--filter", "label=kymo.job=judgetest"])
            .output()
            .unwrap();
        left = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if left.is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("left over: {left}");
}

#[tokio::test]
async fn the_program_is_isolated_from_the_host_and_cannot_run_what_it_writes() {
    if !available() {
        return;
    }
    let src = r##"import os, subprocess, ctypes
libc = ctypes.CDLL(None, use_errno=True)
def denied(nr, *a):
    return libc.syscall(nr, *[ctypes.c_long(x) for x in a]) < 0
ok = []
ok.append(denied(310, 1, 0, 0, 0, 0, 0))
ok.append(denied(101, 16, 1, 0, 0))
ok.append(denied(135, 0x0008))
ok.append(sorted(os.listdir("/sys/class/net")) == ["lo"])
ok.append(os.getuid() == 65534)
ok.append([l.split()[1] for l in open("/proc/self/status") if l.startswith("CapEff")][0] == "0000000000000000")
ok.append(not os.path.exists("/var/run/docker.sock"))
ok.append([p for p in os.listdir("/proc") if p.isdigit()] == ["1"])
open("/tmp/x", "w").write("#!/bin/sh\n")
os.chmod("/tmp/x", 0o755)
try:
    subprocess.run(["/tmp/x"]); ok.append(False)
except PermissionError:
    ok.append(True)
try:
    open("/dev/shm/x", "w"); ok.append(False)
except OSError:
    ok.append(True)
print(all(ok), ok)
"##;
    let report = run(src, "", "True", 5000, 128).await;
    assert!(
        report.cases[0].stdout.starts_with("True"),
        "{:?}",
        report.cases[0]
    );
}
