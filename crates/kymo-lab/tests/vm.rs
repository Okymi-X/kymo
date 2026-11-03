//! The virtual machine runtime, with stand-in `qemu-system` and `qemu-img` scripts that record how
//! they were called and "daemonize" a placeholder process, so the lifecycle, the command line, the
//! port handling and the cleanup are all checked without a hypervisor. Real `tar` unpacks the OVA.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kymo_instances::{InstanceRuntime, RuntimeError, RuntimeStatus};
use kymo_lab::manifest::{Manifest, PortDecl};
use kymo_lab::vm::{Accel, VmConfig, VmRuntime, VmStart, usernet_established};

struct Fixture {
    dir: PathBuf,
    rt: Arc<VmRuntime>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Stand-in processes must not outlive the test.
        if let Ok(entries) = std::fs::read_dir(self.dir.join("work/instances")) {
            for e in entries.flatten() {
                if let Ok(pid) = std::fs::read_to_string(e.path().join("qemu.pid")) {
                    let _ = std::process::Command::new("kill")
                        .args(["-9", pid.trim()])
                        .status();
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write_script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/bash\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Another test thread may fork while this file is still open for writing, and then running it
    // fails with "text file busy" for a moment. Run it once, retrying through that.
    for _ in 0..200 {
        match std::process::Command::new(path).arg("--version").output() {
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            _ => return,
        }
    }
}

fn unique(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kymo-vm-{label}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("images")).unwrap();
    dir
}

/// The stand-ins record every call in `calls.log` and fail when `fail-qemu` exists.
fn fixture(label: &str, tune: impl FnOnce(&mut VmConfig)) -> Fixture {
    let dir = unique(label);
    let log = dir.join("calls.log");
    write_script(
        &dir.join("qemu-img"),
        &format!(
            r#"echo "img $*" >> {log}
case "$1" in
  --version) echo "qemu-img version 0.0 (stand-in)" ;;
  convert) for last; do :; done; src=""; while [ $# -gt 0 ]; do case "$1" in -f|-O) shift;; convert) ;; *) if [ -z "$src" ]; then src="$1"; else dst="$1"; fi;; esac; shift; done; cp "$src" "$dst" ;;
  create) for last; do :; done; : > "$last" ;;
esac"#,
            log = log.display()
        ),
    );
    write_script(
        &dir.join("qemu-system-x86_64"),
        &format!(
            r#"echo "qemu $*" >> {log}
[ "$1" = "--version" ] && {{ echo "QEMU emulator version 0.0 (stand-in)"; exit 0; }}
if [ -e {dir}/fail-qemu ]; then echo "qemu-system-x86_64: could not access KVM kernel module" >&2; exit 1; fi
name=""; pidfile=""
while [ $# -gt 0 ]; do case "$1" in -name) name="${{2%%,*}}"; shift;; -pidfile) pidfile="$2"; shift;; esac; shift; done
( exec -a "$name" sleep 600 >/dev/null 2>&1 </dev/null ) &
child=$!
# Like real -daemonize: only return once the machine is set up (its name is on its command line).
for _ in $(seq 1 100); do grep -q "$name" /proc/$child/cmdline 2>/dev/null && break; sleep 0.02; done
echo $child > "$pidfile""#,
            log = log.display(),
            dir = dir.display()
        ),
    );
    // The warm-up runs are not part of what the tests look at.
    let _ = std::fs::remove_file(&log);
    let mut cfg = VmConfig::new(dir.join("images"), dir.join("work"));
    cfg.qemu = dir
        .join("qemu-system-x86_64")
        .to_string_lossy()
        .into_owned();
    cfg.qemu_img = dir.join("qemu-img").to_string_lossy().into_owned();
    cfg.bind_ip = "127.0.0.1".into();
    cfg.port_range = (45000, 45099);
    cfg.accel = Accel::Tcg;
    cfg.ovmf = None;
    tune(&mut cfg);
    let rt = VmRuntime::new(cfg);
    Fixture { dir, rt }
}

fn make_ova(dir: &Path, name: &str, disk: &str, contents: &str) {
    let work = dir.join("pack");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join(disk), contents).unwrap();
    std::fs::write(work.join("box.ovf"), "<ovf/>").unwrap();
    let status = std::process::Command::new("tar")
        .args([
            "-cf",
            &dir.join("images").join(name).to_string_lossy(),
            "-C",
            &work.to_string_lossy(),
            "box.ovf",
            disk,
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

fn calls(f: &Fixture) -> Vec<String> {
    std::fs::read_to_string(f.dir.join("calls.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn start_of(name: &str) -> VmStart {
    VmStart {
        instance_id: "7".into(),
        name: name.into(),
        artifact: "box".into(),
        file: "box.ova".into(),
        ports: vec![
            PortDecl {
                name: "web".into(),
                port: 80,
            },
            PortDecl {
                name: "ssh".into(),
                port: 22,
            },
        ],
        memory_mb: 1024,
        cpus: 2,
        disk: "sata".into(),
        nic: "e1000".into(),
        firmware: "bios".into(),
        boot_secs: 200,
        tap: None,
    }
}

async fn ready(f: &Fixture) {
    make_ova(&f.dir, "box.ova", "disk.vmdk", "pretend disk");
    f.rt.prepare("box", "box.ova").await.unwrap();
}

#[tokio::test]
async fn an_ova_is_unpacked_once_and_again_only_when_it_changes() {
    let f = fixture("prep", |_| {});
    make_ova(&f.dir, "box.ova", "disk.vmdk", "version one");
    f.rt.prepare("box", "box.ova").await.unwrap();
    let converts = || {
        calls(&f)
            .iter()
            .filter(|c| c.starts_with("img convert"))
            .count()
    };
    assert_eq!(converts(), 1);
    assert!(calls(&f)[0].contains("-f vmdk -O qcow2"), "{:?}", calls(&f));
    f.rt.prepare("box", "box.ova").await.unwrap();
    assert_eq!(
        converts(),
        1,
        "an image that is current is not converted again"
    );
    make_ova(&f.dir, "box.ova", "disk.vmdk", "version two, longer");
    f.rt.prepare("box", "box.ova").await.unwrap();
    assert_eq!(converts(), 2, "a changed image is");

    assert!(
        f.rt.prepare("box", "missing.ova")
            .await
            .unwrap_err()
            .contains("not in")
    );
    std::fs::write(f.dir.join("images/empty.ova"), "").unwrap();
    make_ova(&f.dir, "nodisk.ova", "readme.txt", "no disk in here");
    assert!(
        f.rt.prepare("nodisk", "nodisk.ova")
            .await
            .unwrap_err()
            .contains("no disk image")
    );
    // A bare disk is used without unpacking.
    std::fs::write(f.dir.join("images/raw.img"), "raw bytes").unwrap();
    f.rt.prepare("rawbox", "raw.img").await.unwrap();
    assert!(calls(&f).iter().any(|c| c.contains("-f raw -O qcow2")));
}

#[tokio::test]
async fn a_machine_runs_from_a_throwaway_overlay_with_only_the_listed_ports_forwarded() {
    let f = fixture("start", |_| {});
    ready(&f).await;
    let started =
        f.rt.start_vm(&start_of("kymo-c1-u1-aaaaaa"), 1000)
            .await
            .unwrap();
    assert_eq!(started.handle, "kymo-c1-u1-aaaaaa");
    assert_eq!(started.ready_secs, Some(200));
    assert_eq!(started.extra_ports.len(), 1);
    assert_eq!(
        (
            started.extra_ports[0].name.as_str(),
            started.extra_ports[0].guest
        ),
        ("ssh", 22)
    );
    for p in std::iter::once(started.host_port).chain(started.extra_ports.iter().map(|p| p.host)) {
        assert!((45000..=45099).contains(&p), "{p} is inside the range");
    }
    assert_ne!(started.host_port, started.extra_ports[0].host);

    let log = calls(&f);
    let overlay = log
        .iter()
        .find(|c| c.starts_with("img create"))
        .expect("an overlay was made");
    assert!(
        overlay.contains("-f qcow2 -F qcow2 -b ")
            && overlay.contains("base.qcow2")
            && overlay.ends_with("overlay.qcow2"),
        "{overlay}"
    );
    let qemu = log.iter().find(|c| c.starts_with("qemu ")).unwrap();
    for part in [
        "-m 1024",
        "-smp 2",
        "-machine pc,accel=tcg",
        "-cpu max",
        "restrict=on",
        &format!("hostfwd=tcp:127.0.0.1:{}-:80", started.host_port),
        &format!("hostfwd=tcp:127.0.0.1:{}-:22", started.extra_ports[0].host),
        "e1000,netdev=n0,addr=0x3",
        "acpi-root-pci-hotplug=off",
        "ahci,id=ahci0,addr=0x4",
        "-display none",
        "-daemonize",
        "-pidfile",
        "-name kymo-vm-kymo-c1-u1-aaaaaa,process=kymo-vm-kymo-c1-u1-aaaaaa",
    ] {
        assert!(qemu.contains(part), "missing `{part}` in: {qemu}");
    }
    assert!(
        !qemu.contains("accel=kvm"),
        "forced emulation never asks for KVM"
    );
}

#[test]
fn on_a_lab_network_a_machine_is_a_tap_with_its_own_address_and_nothing_is_forwarded() {
    let f = fixture("tapargs", |_| {});
    let mut v = start_of("kymo-c1-u1-tapped");
    v.tap = Some(kymo_lab::tapnet::TapSpec {
        ip: "10.77.0.42".parse().unwrap(),
        gateway: "10.77.0.33".parse().unwrap(),
        prefix: 27,
    });
    let args =
        f.rt.qemu_args(&v, Path::new("/tmp/x"), &[80, 22], Accel::Kvm)
            .join(" ");
    let ifname = kymo_lab::tapnet::ifname("kymo-c1-u1-tapped");
    assert!(
        args.contains(&format!(
            "-netdev tap,id=n0,ifname={ifname},script=no,downscript=no"
        )),
        "{args}"
    );
    assert!(
        args.contains(&format!(
            "e1000,netdev=n0,addr=0x3,mac={}",
            kymo_lab::tapnet::mac_text(kymo_lab::tapnet::mac("kymo-c1-u1-tapped"))
        )),
        "{args}"
    );
    assert!(
        !args.contains("hostfwd") && !args.contains("restrict=on"),
        "{args}"
    );
}

#[test]
fn a_virtual_machine_is_shown_as_a_machine_and_a_container_as_a_service_unless_the_manifest_says() {
    let m = |json: &str| serde_json::from_str::<kymo_lab::Manifest>(json).unwrap();
    assert!(
        m(r#"{"artifact":"b","runtime":"vm","file":"a.ova","ports":[{"port":80}]}"#)
            .machine_style()
    );
    assert!(!m(r#"{"artifact":"b","port":80}"#).machine_style());
    assert!(m(r#"{"artifact":"b","port":80,"display":"machine"}"#).machine_style());
    assert!(!m(r#"{"artifact":"b","runtime":"vm","file":"a.ova","ports":[{"port":80}],"display":"service"}"#).machine_style());
    assert!(
        m(r#"{"artifact":"b","port":80,"display":"nonsense"}"#)
            .check()
            .is_err()
    );
}

#[tokio::test]
async fn disks_nics_and_firmware_follow_the_manifest() {
    let f = fixture("opts", |c| {
        // Stand-ins for the OVMF files.
        let code = c.work_dir.parent().unwrap().join("OVMF_CODE.fd");
        let vars = c.work_dir.parent().unwrap().join("OVMF_VARS.fd");
        std::fs::write(&code, "code").unwrap();
        std::fs::write(&vars, "vars").unwrap();
        c.ovmf = Some((code, vars));
    });
    ready(&f).await;
    let mut v = start_of("kymo-c2-u1-bbbbbb");
    v.disk = "virtio".into();
    v.nic = "virtio".into();
    v.firmware = "uefi".into();
    f.rt.start_vm(&v, 1).await.unwrap();
    let qemu = calls(&f)
        .into_iter()
        .find(|c| c.starts_with("qemu "))
        .unwrap();
    for part in [
        "virtio-blk-pci,drive=d0,addr=0x4",
        "virtio-net-pci,netdev=n0,addr=0x3",
        "if=pflash,format=raw,unit=0,readonly=on",
        "if=pflash,format=raw,unit=1",
    ] {
        assert!(qemu.contains(part), "missing `{part}` in: {qemu}");
    }
    let vars = std::fs::read_dir(f.dir.join("work/instances"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("vars.fd");
    assert_eq!(
        std::fs::read_to_string(vars).unwrap(),
        "vars",
        "each machine gets its own copy of the UEFI variables"
    );

    // UEFI without firmware installed is refused with the fix in the message.
    let g = fixture("nofw", |c| c.ovmf = None);
    ready(&g).await;
    let mut v = start_of("kymo-c3-u1-cccccc");
    v.firmware = "uefi".into();
    let err = g.rt.start_vm(&v, 1).await.unwrap_err();
    assert!(
        matches!(&err, RuntimeError::Failed(m) if m.contains("ovmf")),
        "{err}"
    );
    assert!(
        std::fs::read_dir(g.dir.join("work/instances")).map_or(true, |mut d| d.next().is_none()),
        "nothing left behind"
    );
}

#[tokio::test]
async fn status_listing_and_stop_follow_the_process_and_clean_up_everything() {
    let f = fixture("life", |_| {});
    ready(&f).await;
    let started =
        f.rt.start_vm(&start_of("kymo-c1-u1-dddddd"), 1000)
            .await
            .unwrap();
    assert_eq!(
        f.rt.status(&started.handle).await.unwrap(),
        RuntimeStatus::Running
    );
    let managed = f.rt.list_managed().await.unwrap();
    assert_eq!(
        (managed[0].handle.as_str(), managed[0].instance_id.as_str()),
        ("kymo-c1-u1-dddddd", "7")
    );
    let pid: u32 = std::fs::read_to_string(
        std::fs::read_dir(f.dir.join("work/instances"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("qemu.pid"),
    )
    .unwrap()
    .trim()
    .parse()
    .unwrap();

    f.rt.stop(&started.handle).await.unwrap();
    assert_eq!(
        f.rt.status(&started.handle).await.unwrap(),
        RuntimeStatus::Missing
    );
    assert!(f.rt.list_managed().await.unwrap().is_empty());
    // A killed process is a zombie for a moment, then gone: either way it has no command line.
    let dead = || std::fs::read(format!("/proc/{pid}/cmdline")).map_or(true, |c| c.is_empty());
    for _ in 0..100 {
        if dead() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(dead(), "the process {pid} is gone");
    assert!(
        std::fs::read_dir(f.dir.join("work/instances"))
            .unwrap()
            .next()
            .is_none(),
        "the directory is gone"
    );
    f.rt.stop(&started.handle)
        .await
        .expect("stopping twice is fine");
    f.rt.stop("../../etc")
        .await
        .expect("a hostile handle is ignored, not followed");

    // A machine whose process died on its own reads as exited, not running.
    let again =
        f.rt.start_vm(&start_of("kymo-c1-u1-eeeeee"), 1000)
            .await
            .unwrap();
    let dir = std::fs::read_dir(f.dir.join("work/instances"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let pid = std::fs::read_to_string(dir.join("qemu.pid")).unwrap();
    std::process::Command::new("kill")
        .args(["-9", pid.trim()])
        .status()
        .unwrap();
    for _ in 0..50 {
        if f.rt.status(&again.handle).await.unwrap() == RuntimeStatus::Exited {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        f.rt.status(&again.handle).await.unwrap(),
        RuntimeStatus::Exited
    );
}

#[tokio::test]
async fn a_start_that_fails_leaves_nothing_behind_and_frees_its_ports() {
    let f = fixture("fail", |c| c.port_range = (45200, 45201));
    ready(&f).await;
    std::fs::write(f.dir.join("fail-qemu"), "").unwrap();
    let err =
        f.rt.start_vm(&start_of("kymo-c1-u1-ffffff"), 1)
            .await
            .unwrap_err();
    assert!(
        matches!(&err, RuntimeError::Failed(m) if m.contains("KVM")),
        "qemu's own words reach the operator: {err}"
    );
    assert!(
        std::fs::read_dir(f.dir.join("work/instances"))
            .unwrap()
            .next()
            .is_none()
    );
    // The two ports of the range are free again, so a start that works can use both.
    std::fs::remove_file(f.dir.join("fail-qemu")).unwrap();
    let ok =
        f.rt.start_vm(&start_of("kymo-c1-u1-gggggg"), 1)
            .await
            .unwrap();
    let mut ports = vec![ok.host_port, ok.extra_ports[0].host];
    ports.sort_unstable();
    assert_eq!(ports, vec![45200, 45201]);
    // And nothing is left for a third port.
    let err =
        f.rt.start_vm(&start_of("kymo-c1-u1-hhhhhh"), 1)
            .await
            .unwrap_err();
    assert!(matches!(err, RuntimeError::Unavailable(m) if m.contains("no free ports")));
}

#[tokio::test]
async fn an_image_that_is_not_ready_is_try_again_later_and_a_new_runtime_finds_running_machines() {
    let f = fixture("adopt", |_| {});
    // Nothing prepared yet.
    let err =
        f.rt.start_vm(&start_of("kymo-c1-u1-iiiiii"), 1)
            .await
            .unwrap_err();
    assert!(
        matches!(&err, RuntimeError::Unavailable(m) if m.contains("not ready")),
        "{err}"
    );

    ready(&f).await;
    let first =
        f.rt.start_vm(&start_of("kymo-c1-u1-jjjjjj"), 1)
            .await
            .unwrap();
    // The lab host restarts: a new runtime over the same directories.
    let mut cfg = f.rt.config().clone_config();
    cfg.port_range = (45000, 45099);
    let again = VmRuntime::new(cfg);
    assert_eq!(
        again.status(&first.handle).await.unwrap(),
        RuntimeStatus::Running,
        "the machine kept running"
    );
    assert_eq!(again.list_managed().await.unwrap().len(), 1);
    // Its ports are still taken, so the next machine gets different ones.
    again.prepare("box", "box.ova").await.unwrap();
    let second = again
        .start_vm(&start_of("kymo-c1-u1-kkkkkk"), 1)
        .await
        .unwrap();
    assert_ne!(second.host_port, first.host_port);
    assert_ne!(second.host_port, first.extra_ports[0].host);
}

#[tokio::test]
async fn the_report_says_what_the_machine_can_do() {
    let f = fixture("report", |_| {});
    let lines = f.rt.report().await.join("\n");
    assert!(lines.contains("qemu: QEMU emulator version 0.0"), "{lines}");
    assert!(lines.contains("qemu-img: qemu-img version 0.0"), "{lines}");
    assert!(
        lines.contains("acceleration: none"),
        "forced emulation is called what it is: {lines}"
    );
    assert!(lines.contains("uefi firmware: not found"), "{lines}");
    let broken = fixture("broken", |c| c.qemu = "/definitely/not/here".into());
    assert!(broken.rt.report().await.join("\n").contains("MISSING"));
}

#[test]
fn qemus_connection_table_tells_a_booted_guest_from_one_that_is_not() {
    let booting = "Hub -1 (n0):\n  Protocol[State]    FD  Source Address  Port   Dest. Address  Port RecvQ SendQ\n  TCP[SYN_SENT]      25       127.0.0.1 40185       10.0.2.15    80     0     0\n  TCP[HOST_FORWARD]  17       127.0.0.1 40185       10.0.2.15    80     0     0\n";
    let up = "  TCP[ESTABLISHED]   26       127.0.0.1 40185       10.0.2.15    80     0     0\n";
    assert!(!usernet_established(booting, 80));
    assert!(usernet_established(&format!("{booting}{up}"), 80));
    assert!(
        !usernet_established(&format!("{booting}{up}"), 22),
        "another port is another service"
    );
    assert!(!usernet_established("", 80));
}

fn vm_manifest(json: &str) -> Result<(), String> {
    serde_json::from_str::<Manifest>(json).unwrap().check()
}

#[test]
fn a_virtual_machine_manifest_is_checked() {
    let ok = r#"{"artifact":"box","runtime":"vm","file":"Box One.ova","ports":[{"name":"web","port":80},{"name":"ssh","port":22}],"memory_mb":1024}"#;
    assert_eq!(vm_manifest(ok), Ok(()));
    for (bad, why) in [
        (
            r#"{"artifact":"box","runtime":"vm","ports":[{"port":80}]}"#,
            "file",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"../etc/passwd.ova","ports":[{"port":80}]}"#,
            "plain file name",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a/b.ova","ports":[{"port":80}]}"#,
            "plain file name",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"notes.txt","ports":[{"port":80}]}"#,
            "ending in",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a.ova"}"#,
            "ports",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a.ova","ports":[{"port":80},{"port":80,"name":"again"}]}"#,
            "twice",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a.ova","ports":[{"port":80,"name":"Web!"}]}"#,
            "not valid",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a.ova","ports":[{"port":80}],"disk":"floppy"}"#,
            "disk is one of",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a.ova","ports":[{"port":80}],"boot_timeout_secs":1}"#,
            "boot_timeout_secs",
        ),
        (
            r#"{"artifact":"box","runtime":"vm","file":"a.ova","ports":[{"port":80}],"env":{"A":"b"}}"#,
            "environment",
        ),
        (
            r#"{"artifact":"c/web:1","runtime":"docker","port":80,"file":"a.ova"}"#,
            "virtual machines",
        ),
        (
            r#"{"artifact":"c/web:1","runtime":"docker","port":80,"ports":[{"port":80}]}"#,
            "virtual machines",
        ),
    ] {
        let err = vm_manifest(bad).unwrap_err();
        assert!(err.contains(why), "{bad} -> {err}");
    }
    let m: Manifest = serde_json::from_str(ok).unwrap();
    assert_eq!(m.primary_port(), 80);
    assert_eq!(m.declared_ports().len(), 2);
}

#[tokio::test]
async fn a_lab_host_runs_a_virtual_machine_through_its_api_from_preparation_to_expiry() {
    use std::sync::atomic::{AtomicI64, Ordering};

    use kymo_instances::{FakeRuntime, Placement, StartRequest};
    use kymo_lab::RemoteRuntime;
    use kymo_lab::server::{self, Lab, LabConfig};

    const TOKEN: &str = "vm-lab-test-token-with-enough-length-1";
    let f = fixture("api", |_| {});
    make_ova(&f.dir, "box.ova", "disk.vmdk", "pretend disk");
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "artifact": "box", "runtime": "vm", "file": "box.ova",
        "ports": [{"name": "web", "port": 80}, {"name": "ssh", "port": 22}],
        "memory_mb": 512, "cpu_millis": 1000, "boot_timeout_secs": 120, "max_ttl_secs": 900
    }))
    .unwrap();
    let mut cfg = LabConfig::new("vm-lab", TOKEN);
    cfg.manifests = vec![manifest];
    let clock = Arc::new(AtomicI64::new(1_800_000_000));
    let c = Arc::clone(&clock);
    let lab = Lab::with_parts(
        cfg,
        Arc::new(FakeRuntime::new()),
        Some(Arc::clone(&f.rt)),
        Arc::new(move || c.load(Ordering::SeqCst)),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(server::serve(
        listener,
        server::router(Arc::clone(&lab)),
        None,
        std::future::pending(),
    ));
    let client = RemoteRuntime::new(&url, TOKEN, None, "127.0.0.1").unwrap();

    let request = |id: &str, name: &str| StartRequest {
        instance_id: id.into(),
        spec: kymo_instances::InstanceSpec {
            image: "box".into(),
            port: 80,
            cpu_millis: 4000,
            memory_mb: 4096,
            pids: 64,
            ttl_secs: 3000,
            env: vec![],
            read_only: false,
        },
        placement: Placement::Published,
        name: Some(name.into()),
        meta: vec![("event".into(), "1".into())],
    };

    // The health report says what this lab host can run and how fast.
    let health = client.health().await.unwrap();
    assert_eq!(health.runtimes, vec!["docker".to_owned(), "vm".to_owned()]);
    assert_eq!(health.accel.as_deref(), Some("tcg"));

    // Before the image is prepared it says so, and a start is "try again shortly", creating nothing.
    let before = client.artifacts().await.unwrap();
    assert_eq!(
        (before[0].ready, before[0].ports.len()),
        (false, 2),
        "{before:?}"
    );
    let err = client
        .start(&request("1", "kymo-c1-u1-aaaaaa"))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, RuntimeError::Unavailable(m) if m.contains("not ready")),
        "{err}"
    );
    assert!(client.instances().await.unwrap().is_empty());

    // Preparing happens in the background; wait for it, then the artifact is ready.
    lab.prepare_images();
    for _ in 0..200 {
        if client.artifacts().await.unwrap()[0].ready {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let after = client.artifacts().await.unwrap();
    assert!(after[0].ready && after[0].note.is_none(), "{after:?}");

    // Environment cannot be set on a machine, and a start asks for more than the manifest allows.
    let mut with_env = request("2", "kymo-c1-u1-bbbbbb");
    with_env.spec.env = vec![("FLAG".into(), "x".into())];
    assert!(
        matches!(client.start(&with_env).await, Err(RuntimeError::Failed(m)) if m.contains("virtual machine"))
    );
    let started = client
        .start(&request("3", "kymo-c1-u1-cccccc"))
        .await
        .unwrap();
    assert_eq!(
        started.ready_secs,
        Some(120),
        "the manifest's boot time reaches the platform"
    );
    assert_eq!(started.extra_ports.len(), 1);
    let qemu = calls(&f)
        .into_iter()
        .find(|c| c.starts_with("qemu "))
        .unwrap();
    assert!(
        qemu.contains("-m 512") && qemu.contains("-smp 1"),
        "the manifest's limits win over the request: {qemu}"
    );

    let listed = client.instances().await.unwrap();
    assert_eq!(
        (
            listed[0].runtime.as_str(),
            listed[0].status.as_str(),
            listed[0].ports.len()
        ),
        ("vm", "running", 1)
    );
    assert_eq!(
        listed[0].expires_at - listed[0].created_at,
        900,
        "the machine's time limit is the manifest's"
    );

    // No control socket in the stand-in, so the guest cannot be confirmed up: not ready, and the lab
    // host says it checked (vouched) rather than guessing from a connection.
    let ready = reqwest::get_json(
        &url,
        TOKEN,
        &format!("/v1/ready?port={}", started.host_port),
    )
    .await;
    assert_eq!(
        ready,
        serde_json::json!({ "ready": false, "vouched": true })
    );
    assert!(!client.reachable(started.host_port).await);
    assert_eq!(
        reqwest::get_json(&url, TOKEN, "/v1/ready?port=1").await,
        serde_json::json!({ "ready": true, "vouched": false })
    );

    // Restart replaces the process under the same name.
    let again = reqwest::post_json(&url, TOKEN, "/v1/instances/kymo-c1-u1-cccccc/restart").await;
    assert_eq!(again["name"], "kymo-c1-u1-cccccc");
    assert_eq!(again["runtime"], "vm");
    assert_eq!(client.instances().await.unwrap().len(), 1);

    // Expiry is the lab host's own business.
    clock.fetch_add(901 + 5, Ordering::SeqCst);
    assert_eq!(lab.cleanup().await, vec!["kymo-c1-u1-cccccc".to_owned()]);
    assert!(f.rt.list_managed().await.unwrap().is_empty());

    // And what the platform no longer lists is removed, machines included.
    client
        .start(&request("4", "kymo-c1-u1-dddddd"))
        .await
        .unwrap();
    assert_eq!(
        client.reconcile(vec![], Some(0)).await.unwrap(),
        vec!["kymo-c1-u1-dddddd".to_owned()]
    );
    assert!(f.rt.list_managed().await.unwrap().is_empty());
}

/// Small helpers for the two calls the client does not wrap.
mod reqwest {
    pub async fn get_json(url: &str, token: &str, path: &str) -> serde_json::Value {
        ::reqwest::Client::new()
            .get(format!("{url}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
    pub async fn post_json(url: &str, token: &str, path: &str) -> serde_json::Value {
        ::reqwest::Client::new()
            .post(format!("{url}{path}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}
