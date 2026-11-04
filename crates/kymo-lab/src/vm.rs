//! Virtual machines on a lab host: QEMU, with KVM acceleration when the machine has it and plain
//! emulation otherwise.
//!
//! * An image (an OVA from VirtualBox or VMware, or a bare disk) is unpacked and converted **once**
//!   into a base image that is never written to.
//! * Every instance gets a throwaway copy-on-write overlay of it, so a player can wreck the machine
//!   and the next start is pristine, and starting costs almost nothing.
//! * Networking is QEMU's user mode with `restrict=on`: the guest reaches neither the host nor the
//!   internet. Only the ports the manifest lists are forwarded in, on ports from a range.
//! * Machines are ordinary processes started with `-daemonize`, found again after a restart by a
//!   small metadata file in the instance directory, and killed when the instance ends.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use kymo_instances::{
    ExtraPort, InstanceRuntime, Managed, RuntimeError, RuntimeStatus, StartRequest, Started,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::process::Command;

use crate::manifest::{Manifest, PortDecl};

const PREPARE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const TOOL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    /// KVM when `/dev/kvm` can be used, otherwise TCG.
    Auto,
    Kvm,
    /// Plain emulation: works anywhere, many times slower.
    Tcg,
}

impl Accel {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "auto" | "" => Ok(Accel::Auto),
            "kvm" => Ok(Accel::Kvm),
            "tcg" => Ok(Accel::Tcg),
            other => Err(format!("the accelerator is auto, kvm or tcg, not {other}")),
        }
    }
}

/// Whether this process may use `/dev/kvm`.
pub fn kvm_usable() -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}

pub struct VmConfig {
    /// Where the OVA and disk files live.
    pub images_dir: PathBuf,
    /// Prepared base images and running instances live under here.
    pub work_dir: PathBuf,
    pub qemu: String,
    pub qemu_img: String,
    pub tar: String,
    /// Address forwarded ports are published on.
    pub bind_ip: String,
    /// Forwarded ports are taken from this inclusive range.
    pub port_range: (u16, u16),
    pub accel: Accel,
    /// UEFI firmware, found on first use when not set.
    pub ovmf: Option<(PathBuf, PathBuf)>,
}

impl VmConfig {
    pub fn clone_config(&self) -> Self {
        Self {
            images_dir: self.images_dir.clone(),
            work_dir: self.work_dir.clone(),
            qemu: self.qemu.clone(),
            qemu_img: self.qemu_img.clone(),
            tar: self.tar.clone(),
            bind_ip: self.bind_ip.clone(),
            port_range: self.port_range,
            accel: self.accel,
            ovmf: self.ovmf.clone(),
        }
    }

    pub fn new(images_dir: impl Into<PathBuf>, work_dir: impl Into<PathBuf>) -> Self {
        Self {
            images_dir: images_dir.into(),
            work_dir: work_dir.into(),
            qemu: "qemu-system-x86_64".into(),
            qemu_img: "qemu-img".into(),
            tar: "tar".into(),
            bind_ip: "0.0.0.0".into(),
            port_range: (40000, 40999),
            accel: Accel::Auto,
            ovmf: find_ovmf(),
        }
    }
}

/// Where distributions keep OVMF, as (code, variables) pairs.
fn find_ovmf() -> Option<(PathBuf, PathBuf)> {
    const PAIRS: &[(&str, &str)] = &[
        (
            "/usr/share/edk2/x64/OVMF_CODE.4m.fd",
            "/usr/share/edk2/x64/OVMF_VARS.4m.fd",
        ),
        (
            "/usr/share/OVMF/OVMF_CODE_4M.fd",
            "/usr/share/OVMF/OVMF_VARS_4M.fd",
        ),
        (
            "/usr/share/OVMF/OVMF_CODE.fd",
            "/usr/share/OVMF/OVMF_VARS.fd",
        ),
        (
            "/usr/share/edk2/ovmf/OVMF_CODE.fd",
            "/usr/share/edk2/ovmf/OVMF_VARS.fd",
        ),
        (
            "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd",
            "/usr/share/edk2-ovmf/x64/OVMF_VARS.fd",
        ),
    ];
    PAIRS
        .iter()
        .find(|(c, v)| Path::new(c).exists() && Path::new(v).exists())
        .map(|(c, v)| (PathBuf::from(c), PathBuf::from(v)))
}

/// Everything needed to start one machine, decided by the lab host from a manifest.
#[derive(Debug, Clone)]
pub struct VmStart {
    pub instance_id: String,
    pub name: String,
    pub artifact: String,
    pub file: String,
    pub ports: Vec<PortDecl>,
    pub memory_mb: u32,
    pub cpus: u32,
    pub disk: String,
    pub nic: String,
    pub firmware: String,
    pub boot_secs: u32,
    /// Put the machine on a lab network as a machine of its own (a tap on the network's bridge
    /// and its reserved address from DHCP) instead of forwarding ports on the lab host.
    pub tap: Option<crate::tapnet::TapSpec>,
}

impl VmStart {
    /// From a manifest and the limits already applied to the request.
    pub fn from_manifest(
        m: &Manifest,
        instance_id: &str,
        name: &str,
        memory_mb: u32,
        cpu_millis: u32,
    ) -> Self {
        Self {
            instance_id: instance_id.to_owned(),
            name: name.to_owned(),
            artifact: m.artifact.clone(),
            file: m.file.clone().unwrap_or_default(),
            ports: m.declared_ports(),
            memory_mb,
            cpus: cpu_millis.div_ceil(1000).clamp(1, 16),
            disk: m.disk.clone().unwrap_or_else(|| "sata".into()),
            nic: m.nic.clone().unwrap_or_else(|| "e1000".into()),
            firmware: m.firmware.clone().unwrap_or_else(|| "bios".into()),
            boot_secs: m.boot_timeout_secs.unwrap_or(180),
            tap: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Meta {
    instance_id: String,
    name: String,
    artifact: String,
    created_at: i64,
    ports: Vec<ExtraPort>,
    /// How it sits on a lab network, so it can be cleaned up and served again after a restart.
    #[serde(default)]
    tap: Option<TapMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TapMeta {
    ifname: String,
    /// The bridge the tap is on, which DHCP listens on.
    #[serde(default)]
    bridge: String,
    ip: std::net::Ipv4Addr,
    gateway: std::net::Ipv4Addr,
    prefix: u8,
}

#[derive(Debug, Clone)]
enum Prep {
    Preparing,
    Ready,
    Failed(String),
}

pub struct VmRuntime {
    cfg: VmConfig,
    prep: Mutex<HashMap<String, Prep>>,
    reserved: Mutex<HashSet<u16>>,
    control: tokio::sync::Mutex<()>,
    /// The DHCP server of each machine that is on a lab network, by machine name.
    dhcp: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn unavailable(text: impl Into<String>) -> RuntimeError {
    RuntimeError::Unavailable(text.into())
}

fn failed(text: impl Into<String>) -> RuntimeError {
    RuntimeError::Failed(text.into())
}

/// Instance names reach the file system, so only the safe ones do.
fn valid_handle(handle: &str) -> bool {
    crate::naming::valid_name(handle)
}

fn slug(artifact: &str) -> String {
    let hash = Sha256::digest(artifact.as_bytes());
    let short: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{}-{short}", crate::naming::sanitize(artifact, 40))
}

/// A stamp that changes when the source image does.
fn stamp_of(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Ok(format!("{}:{modified}", meta.len()))
}

fn disk_format(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".vmdk") {
        "vmdk"
    } else if lower.ends_with(".vdi") {
        "vdi"
    } else if lower.ends_with(".qcow2") {
        "qcow2"
    } else {
        "raw"
    }
}

async fn run(program: &str, args: &[&str], limit: Duration) -> Result<String, String> {
    let output = tokio::time::timeout(
        limit,
        Command::new(program).args(args).kill_on_drop(true).output(),
    )
    .await
    .map_err(|_| format!("{program} took too long"))?
    .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "{program} failed: {}",
            err.lines().last().unwrap_or("no message").trim()
        ))
    }
}

impl VmRuntime {
    pub fn new(cfg: VmConfig) -> Arc<Self> {
        let rt = Arc::new(Self {
            cfg,
            prep: Mutex::default(),
            reserved: Mutex::default(),
            control: tokio::sync::Mutex::new(()),
            dhcp: Mutex::default(),
        });
        // Ports of machines that survived a restart stay taken.
        for meta in rt.metas() {
            lock(&rt.reserved).extend(meta.ports.iter().map(|p| p.host));
        }
        rt
    }

    pub fn config(&self) -> &VmConfig {
        &self.cfg
    }

    fn instances_dir(&self) -> PathBuf {
        self.cfg.work_dir.join("instances")
    }

    /// An instance's directory. Named by a short hash so the control socket inside it fits the
    /// 108 bytes a Unix socket path may have, however long the instance name is.
    fn instance_dir(&self, name: &str) -> PathBuf {
        let hash = Sha256::digest(name.as_bytes());
        let short: String = hash.iter().take(6).map(|b| format!("{b:02x}")).collect();
        self.instances_dir().join(short)
    }

    fn base_dir(&self, artifact: &str) -> PathBuf {
        self.cfg.work_dir.join("base").join(slug(artifact))
    }

    /// The accelerator a start will use, or why none is available.
    pub fn accel(&self) -> Result<Accel, String> {
        match self.cfg.accel {
            Accel::Tcg => Ok(Accel::Tcg),
            Accel::Kvm if kvm_usable() => Ok(Accel::Kvm),
            Accel::Kvm => Err("KVM was asked for, but /dev/kvm is not usable by this user (is it in the kvm group?)".into()),
            Accel::Auto => Ok(if kvm_usable() { Accel::Kvm } else { Accel::Tcg }),
        }
    }

    /// What this machine can do, for `kymo-lab check` and the health report.
    pub async fn report(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (label, program) in [("qemu", &self.cfg.qemu), ("qemu-img", &self.cfg.qemu_img)] {
            match run(program, &["--version"], TOOL_TIMEOUT).await {
                Ok(out) => lines.push(format!(
                    "{label}: {}",
                    out.lines().next().unwrap_or("").trim()
                )),
                Err(e) => lines.push(format!("{label}: MISSING ({e})")),
            }
        }
        lines.push(match self.accel() {
            Ok(Accel::Kvm) => "acceleration: KVM".into(),
            Ok(_) => "acceleration: none, plain QEMU emulation (slow; add the service user to the kvm group for KVM)".into(),
            Err(e) => format!("acceleration: {e}"),
        });
        lines.push(match &self.cfg.ovmf {
            Some((code, _)) => format!("uefi firmware: {}", code.display()),
            None => {
                "uefi firmware: not found (only bios machines; install ovmf or edk2-ovmf)".into()
            }
        });
        lines.push(format!("images: {}", self.cfg.images_dir.display()));
        lines
    }

    // ---------------------------------------------------------------- preparing images

    /// Whether an image is ready, and a sentence about it when it is not.
    pub fn prepared(&self, artifact: &str) -> (bool, Option<String>) {
        match lock(&self.prep).get(artifact) {
            Some(Prep::Ready) => (true, None),
            Some(Prep::Preparing) => (
                false,
                Some("being unpacked, this takes a minute or two the first time".into()),
            ),
            Some(Prep::Failed(e)) => (false, Some(e.clone())),
            None => (false, Some("not prepared yet".into())),
        }
    }

    /// Starts preparing an image in the background if it is not ready. Returns at once.
    pub fn spawn_prepare(self: &Arc<Self>, artifact: &str, file: &str) {
        {
            let mut prep = lock(&self.prep);
            if matches!(prep.get(artifact), Some(Prep::Preparing | Prep::Ready)) {
                return;
            }
            prep.insert(artifact.to_owned(), Prep::Preparing);
        }
        let rt = Arc::clone(self);
        let (artifact, file) = (artifact.to_owned(), file.to_owned());
        tokio::spawn(async move {
            // `prepare` records the outcome itself.
            if let Err(e) = rt.prepare(&artifact, &file).await {
                tracing::warn!("preparing {artifact}: {e}");
            }
        });
    }

    /// Unpacks and converts an image into its base, once. Safe to call again: it does nothing when
    /// the base is current.
    pub async fn prepare(&self, artifact: &str, file: &str) -> Result<(), String> {
        lock(&self.prep).insert(artifact.to_owned(), Prep::Preparing);
        let result = self.prepare_files(artifact, file).await;
        lock(&self.prep).insert(
            artifact.to_owned(),
            match &result {
                Ok(()) => Prep::Ready,
                Err(e) => Prep::Failed(e.clone()),
            },
        );
        result
    }

    async fn prepare_files(&self, artifact: &str, file: &str) -> Result<(), String> {
        let source = self.cfg.images_dir.join(file);
        if !source.is_file() {
            return Err(format!(
                "the image {file} is not in {}",
                self.cfg.images_dir.display()
            ));
        }
        let stamp = stamp_of(&source)?;
        let dir = self.base_dir(artifact);
        let (base, stamp_file) = (dir.join("base.qcow2"), dir.join("source.stamp"));
        if base.is_file() && std::fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp) {
            return Ok(());
        }
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let work = dir.join("work");
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;

        let source_str = source.to_string_lossy().into_owned();
        let (disk, format) = if file.to_ascii_lowercase().ends_with(".ova") {
            let listing = run(&self.cfg.tar, &["-tf", &source_str], PREPARE_TIMEOUT).await?;
            let entry = listing
                .lines()
                .map(str::trim)
                .find(|l| {
                    let l = l.to_ascii_lowercase();
                    !l.contains('/')
                        && (l.ends_with(".vmdk")
                            || l.ends_with(".vdi")
                            || l.ends_with(".qcow2")
                            || l.ends_with(".img"))
                })
                .ok_or("the OVA has no disk image in it")?
                .to_owned();
            run(
                &self.cfg.tar,
                &[
                    "-xf",
                    &source_str,
                    "-C",
                    &work.to_string_lossy(),
                    "--",
                    &entry,
                ],
                PREPARE_TIMEOUT,
            )
            .await?;
            (work.join(&entry), disk_format(&entry))
        } else {
            (source.clone(), disk_format(file))
        };
        let part = dir.join("base.qcow2.part");
        run(
            &self.cfg.qemu_img,
            &[
                "convert",
                "-f",
                format,
                "-O",
                "qcow2",
                &disk.to_string_lossy(),
                &part.to_string_lossy(),
            ],
            PREPARE_TIMEOUT,
        )
        .await?;
        std::fs::rename(&part, &base).map_err(|e| e.to_string())?;
        std::fs::write(&stamp_file, stamp).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&work);
        Ok(())
    }

    // ---------------------------------------------------------------- starting

    /// The qemu command line for a machine. Pure, so it can be checked without running anything.
    pub fn qemu_args(
        &self,
        v: &VmStart,
        dir: &Path,
        host_ports: &[u16],
        accel: Accel,
    ) -> Vec<String> {
        let name = format!("kymo-vm-{}", v.name);
        let mut args: Vec<String> = Vec::new();
        let mut push = |items: &[&str]| args.extend(items.iter().map(|s| (*s).to_owned()));
        push(&["-name", &format!("{name},process={name}")]);
        push(&[
            "-nodefaults",
            "-no-user-config",
            "-display",
            "none",
            "-vga",
            "std",
        ]);
        match accel {
            Accel::Kvm => push(&["-machine", "pc,accel=kvm", "-cpu", "host"]),
            _ => push(&["-machine", "pc,accel=tcg", "-cpu", "max"]),
        }
        push(&["-smp", &v.cpus.to_string(), "-m", &v.memory_mb.to_string()]);
        if v.firmware == "uefi"
            && let Some((code, _)) = &self.cfg.ovmf
        {
            push(&[
                "-drive",
                &format!(
                    "if=pflash,format=raw,unit=0,readonly=on,file={}",
                    code.display()
                ),
            ]);
            push(&[
                "-drive",
                &format!(
                    "if=pflash,format=raw,unit=1,file={}",
                    dir.join("vars.fd").display()
                ),
            ]);
        }
        // Guests exported from VirtualBox name their network card after its PCI slot (enp0s3) and
        // configure that name. QEMU would name it by ACPI slot (ens3) and the guest would never
        // bring it up, so the card goes in slot 3 and slot naming is switched off: same name.
        push(&["-global", "PIIX4_PM.acpi-root-pci-hotplug=off"]);
        push(&[
            "-global",
            "PIIX4_PM.acpi-pci-hotplug-with-bridge-support=off",
        ]);
        let overlay = dir.join("overlay.qcow2");
        match v.disk.as_str() {
            "virtio" => {
                push(&[
                    "-drive",
                    &format!("file={},format=qcow2,if=none,id=d0", overlay.display()),
                ]);
                push(&["-device", "virtio-blk-pci,drive=d0,addr=0x4"]);
            }
            "ide" => push(&[
                "-drive",
                &format!("file={},format=qcow2,if=ide", overlay.display()),
            ]),
            _ => {
                push(&["-device", "ahci,id=ahci0,addr=0x4"]);
                push(&[
                    "-drive",
                    &format!("file={},format=qcow2,if=none,id=d0", overlay.display()),
                ]);
                push(&["-device", "ide-hd,drive=d0,bus=ahci0.0"]);
            }
        }
        if let Some(_tap) = &v.tap {
            // On a lab network as a machine of its own: every port is open on its address. The
            // network has no route out, so the guest still cannot reach the internet.
            push(&[
                "-netdev",
                &format!(
                    "tap,id=n0,ifname={},script=no,downscript=no",
                    crate::tapnet::ifname(&v.name)
                ),
            ]);
        } else {
            // User mode networking with restrict=on: the guest cannot reach the host or the
            // internet, and only the forwarded ports lead in.
            let forwards: String = v
                .ports
                .iter()
                .zip(host_ports)
                .map(|(p, host)| format!(",hostfwd=tcp:{}:{host}-:{}", self.cfg.bind_ip, p.port))
                .collect();
            push(&["-netdev", &format!("user,id=n0,restrict=on{forwards}")]);
        }
        let nic = match v.nic.as_str() {
            "virtio" => "virtio-net-pci",
            "rtl8139" => "rtl8139",
            _ => "e1000",
        };
        let mac = if v.tap.is_some() {
            format!(
                ",mac={}",
                crate::tapnet::mac_text(crate::tapnet::mac(&v.name))
            )
        } else {
            String::new()
        };
        push(&["-device", &format!("{nic},netdev=n0,addr=0x3{mac}")]);
        // A control socket per machine (QMP): used for screenshots and to ask whether the guest
        // has accepted a connection. Skipped when the path would not fit a Unix socket.
        let control = dir.join("qmp.sock");
        if control.as_os_str().len() < 100 {
            push(&[
                "-qmp",
                &format!("unix:{},server=on,wait=off", control.display()),
            ]);
        }
        push(&[
            "-daemonize",
            "-pidfile",
            &dir.join("qemu.pid").to_string_lossy(),
        ]);
        args
    }

    fn allocate_ports(&self, count: usize, bind: &str) -> Result<Vec<u16>, RuntimeError> {
        let (lo, hi) = self.cfg.port_range;
        let span = u32::from(hi.saturating_sub(lo)) + 1;
        let offset = rand::random::<u32>() % span;
        let mut reserved = lock(&self.reserved);
        let mut chosen = Vec::new();
        for i in 0..span {
            if chosen.len() == count {
                break;
            }
            let port = lo + u16::try_from((offset + i) % span).unwrap_or(0);
            if reserved.contains(&port) {
                continue;
            }
            // Free right now? Binding is the honest test.
            if std::net::TcpListener::bind((bind, port)).is_ok() {
                reserved.insert(port);
                chosen.push(port);
            }
        }
        if chosen.len() < count {
            for p in &chosen {
                reserved.remove(p);
            }
            return Err(unavailable(
                "no free ports are left for virtual machines on this lab host",
            ));
        }
        Ok(chosen)
    }

    fn release(&self, ports: &[u16]) {
        let mut reserved = lock(&self.reserved);
        for p in ports {
            reserved.remove(p);
        }
    }

    pub async fn start_vm(&self, v: &VmStart, now: i64) -> Result<Started, RuntimeError> {
        if !valid_handle(&v.name) {
            return Err(failed("the name is not valid"));
        }
        if v.tap.is_none() && v.ports.is_empty() {
            return Err(failed(
                "this machine lists no ports, so it can only run on a lab network (the VPN): there is nothing to publish",
            ));
        }
        let accel = self.accel().map_err(unavailable)?;
        // The image must be ready; if it is not, begin and tell the platform to come back.
        let artifact_ready = self.prepared(&v.artifact).0;
        if !artifact_ready {
            return Err(unavailable(match self.prepared(&v.artifact).1 {
                Some(note) if note != "not prepared yet" => {
                    format!("the image {} is not ready: {note}", v.artifact)
                }
                _ => format!("the image {} is not ready yet", v.artifact),
            }));
        }
        let base = self.base_dir(&v.artifact).join("base.qcow2");
        let dir = self.instance_dir(&v.name);
        if dir.exists() {
            return Err(failed("an instance with that name already exists"));
        }
        std::fs::create_dir_all(&dir).map_err(|e| failed(format!("{}: {e}", dir.display())))?;
        // On a lab network the machine has an address of its own and needs no forwarded ports.
        let host_ports = if v.tap.is_some() {
            v.ports.iter().map(|p| p.port).collect()
        } else {
            match self.allocate_ports(v.ports.len(), &self.cfg.bind_ip) {
                Ok(p) => p,
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err(e);
                }
            }
        };
        let result = self.launch(v, &dir, &base, &host_ports, accel, now).await;
        if result.is_err() {
            if v.tap.is_none() {
                self.release(&host_ports);
            } else {
                self.drop_net(&v.name).await;
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
        result
    }

    async fn launch(
        &self,
        v: &VmStart,
        dir: &Path,
        base: &Path,
        host_ports: &[u16],
        accel: Accel,
        now: i64,
    ) -> Result<Started, RuntimeError> {
        let overlay = dir.join("overlay.qcow2");
        run(
            &self.cfg.qemu_img,
            &[
                "create",
                "-q",
                "-f",
                "qcow2",
                "-F",
                "qcow2",
                "-b",
                &base.to_string_lossy(),
                &overlay.to_string_lossy(),
            ],
            TOOL_TIMEOUT,
        )
        .await
        .map_err(failed)?;
        if v.firmware == "uefi" {
            let (_, vars) = self.cfg.ovmf.as_ref().ok_or_else(|| {
                failed("this lab host has no UEFI firmware (install ovmf or edk2-ovmf)")
            })?;
            std::fs::copy(vars, dir.join("vars.fd"))
                .map_err(|e| failed(format!("uefi variables: {e}")))?;
        }
        let tap = match &v.tap {
            Some(t) => Some(self.plug(&v.name, t).await.map_err(failed)?),
            None => None,
        };
        let args = self.qemu_args(v, dir, host_ports, accel);
        let log = std::fs::File::create(dir.join("qemu.log")).map_err(|e| failed(e.to_string()))?;
        let log2 = log.try_clone().map_err(|e| failed(e.to_string()))?;
        let status = tokio::time::timeout(
            TOOL_TIMEOUT,
            Command::new(&self.cfg.qemu)
                .args(&args)
                .stdin(std::process::Stdio::null())
                .stdout(log)
                .stderr(log2)
                .kill_on_drop(true)
                .status(),
        )
        .await
        .map_err(|_| unavailable("qemu took too long to start"))?
        .map_err(|e| unavailable(format!("cannot run {}: {e}", self.cfg.qemu)))?;
        if !status.success() {
            let text = std::fs::read_to_string(dir.join("qemu.log")).unwrap_or_default();
            let line = text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("no message");
            return Err(failed(format!("qemu did not start: {}", line.trim())));
        }
        let ports: Vec<ExtraPort> = v
            .ports
            .iter()
            .zip(host_ports)
            .map(|(p, host)| ExtraPort {
                name: p.name.clone(),
                guest: p.port,
                host: *host,
            })
            .collect();
        let meta = Meta {
            instance_id: v.instance_id.clone(),
            name: v.name.clone(),
            artifact: v.artifact.clone(),
            created_at: now,
            ports: ports.clone(),
            tap: tap.clone().map(|(t, bridge)| TapMeta {
                ifname: crate::tapnet::ifname(&v.name),
                bridge,
                ip: t.ip,
                gateway: t.gateway,
                prefix: t.prefix,
            }),
        };
        std::fs::write(
            dir.join("meta.json"),
            serde_json::to_vec_pretty(&meta).map_err(|e| failed(e.to_string()))?,
        )
        .map_err(|e| failed(e.to_string()))?;
        // A machine on a lab network may list no ports: it is reached at its own address.
        let mut started = Started::new(
            v.name.clone(),
            ports.first().map_or(0, |p| p.host),
            tap.map(|(t, _)| t.ip.to_string()),
        );
        started.extra_ports = ports.iter().skip(1).cloned().collect();
        started.ready_secs = Some(v.boot_secs);
        Ok(started)
    }

    /// Puts a machine on its lab network: the tap on the bridge, and DHCP for its one address.
    async fn plug(
        &self,
        name: &str,
        tap: &crate::tapnet::TapSpec,
    ) -> Result<(crate::tapnet::TapSpec, String), String> {
        let ifname = crate::tapnet::ifname(name);
        let (bridge, prefix) = crate::tapnet::bridge_for(tap.gateway).await?;
        // The network's own size, as the bridge reports it.
        let tap = crate::tapnet::TapSpec { prefix, ..*tap };
        crate::tapnet::create(&ifname, &bridge).await?;
        match crate::tapnet::serve(&bridge, crate::tapnet::mac(name), tap) {
            Ok(task) => {
                lock(&self.dhcp).insert(name.to_owned(), task);
                Ok((tap, bridge))
            }
            Err(e) => {
                crate::tapnet::remove(&ifname).await;
                Err(e)
            }
        }
    }

    /// Takes a machine off its lab network: DHCP stops and the tap goes.
    async fn drop_net(&self, name: &str) {
        if let Some(task) = lock(&self.dhcp).remove(name) {
            task.abort();
        }
        crate::tapnet::remove(&crate::tapnet::ifname(name)).await;
    }

    /// Answers DHCP again for machines that survived a restart of the lab host (their taps and
    /// QEMU processes stay up). Called once when the lab host starts.
    pub fn revive_net(&self) {
        for meta in self.metas() {
            let Some(t) = meta.tap else {
                continue;
            };
            let alive = self
                .pid_of(&self.instance_dir(&meta.name))
                .is_some_and(|pid| self.alive(&meta.name, pid));
            if !alive || lock(&self.dhcp).contains_key(&meta.name) {
                continue;
            }
            let spec = crate::tapnet::TapSpec {
                ip: t.ip,
                gateway: t.gateway,
                prefix: t.prefix,
            };
            if t.bridge.is_empty() {
                continue;
            }
            match crate::tapnet::serve(&t.bridge, crate::tapnet::mac(&meta.name), spec) {
                Ok(task) => {
                    lock(&self.dhcp).insert(meta.name.clone(), task);
                }
                Err(e) => tracing::warn!("{}: {e}", meta.name),
            }
        }
    }

    /// Runs one QMP command against a machine and returns its `return` value. One at a time: a
    /// QMP socket serves a single client.
    async fn qmp(
        &self,
        handle: &str,
        command: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, RuntimeError> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        if !valid_handle(handle) {
            return Err(failed("no such machine"));
        }
        let socket = self.instance_dir(handle).join("qmp.sock");
        let _one_at_a_time = self.control.lock().await;
        let io = async {
            let stream = tokio::net::UnixStream::connect(&socket)
                .await
                .map_err(|e| unavailable(format!("the machine is not running: {e}")))?;
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            // The greeting, then the handshake QMP requires before any command.
            lines.next_line().await.map_err(|e| failed(e.to_string()))?;
            write
                .write_all(b"{\"execute\":\"qmp_capabilities\"}\n")
                .await
                .map_err(|e| failed(e.to_string()))?;
            let request = serde_json::json!({ "execute": command, "arguments": arguments });
            let mut sent = false;
            while let Some(line) = lines.next_line().await.map_err(|e| failed(e.to_string()))? {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                if value.get("event").is_some() {
                    continue;
                }
                if !sent {
                    // The handshake was answered.
                    write
                        .write_all(format!("{request}\n").as_bytes())
                        .await
                        .map_err(|e| failed(e.to_string()))?;
                    sent = true;
                    continue;
                }
                return match value.get("error") {
                    Some(e) => Err(failed(
                        e["desc"]
                            .as_str()
                            .unwrap_or("qemu refused the command")
                            .to_owned(),
                    )),
                    None => Ok(value
                        .get("return")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)),
                };
            }
            Err(failed("the machine's control socket closed"))
        };
        tokio::time::timeout(Duration::from_secs(10), io)
            .await
            .map_err(|_| unavailable("the machine's control socket did not answer"))?
    }

    /// A PNG of the machine's screen, for an organizer who needs to see what the guest is doing
    /// (stuck at a boot prompt, no network, and so on).
    pub async fn screenshot(&self, handle: &str) -> Result<Vec<u8>, RuntimeError> {
        let shot = self.instance_dir(handle).join("screen.png");
        let _ = std::fs::remove_file(&shot);
        self.qmp(
            handle,
            "screendump",
            serde_json::json!({ "filename": shot.to_string_lossy(), "format": "png" }),
        )
        .await?;
        let png = std::fs::read(&shot)
            .map_err(|e| failed(format!("the screenshot was not made: {e}")))?;
        let _ = std::fs::remove_file(&shot);
        Ok(png)
    }

    /// Whether the guest has accepted a connection on `guest_port`. QEMU accepts the host side of a
    /// forwarded port at once, even while the guest is still booting, so connecting says nothing;
    /// QEMU's own table of forwarded connections does: a connection the guest took is
    /// ESTABLISHED, one it has not answered is stuck in SYN_SENT.
    pub async fn guest_ready(&self, handle: &str, host_port: u16, guest_port: u16) -> bool {
        // Make a connection so there is something to look at, and keep it open while looking.
        let Ok(Ok(probe)) = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(("127.0.0.1", host_port)),
        )
        .await
        else {
            return false;
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        let table = self
            .qmp(
                handle,
                "human-monitor-command",
                serde_json::json!({ "command-line": "info usernet" }),
            )
            .await;
        drop(probe);
        let Ok(serde_json::Value::String(text)) = table else {
            return false;
        };
        usernet_established(&text, guest_port)
    }

    // ---------------------------------------------------------------- finding and ending

    fn metas(&self) -> Vec<Meta> {
        let Ok(entries) = std::fs::read_dir(self.instances_dir()) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|e| std::fs::read_to_string(e.path().join("meta.json")).ok())
            .filter_map(|t| serde_json::from_str::<Meta>(&t).ok())
            .collect()
    }

    fn pid_of(&self, dir: &Path) -> Option<u32> {
        std::fs::read_to_string(dir.join("qemu.pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// A pid is only ours if the process still has this instance's name on its command line, so a
    /// recycled pid is never mistaken for it.
    fn alive(&self, name: &str, pid: u32) -> bool {
        std::fs::read(format!("/proc/{pid}/cmdline"))
            .is_ok_and(|raw| String::from_utf8_lossy(&raw).contains(&format!("kymo-vm-{name}")))
    }
}

#[async_trait]
impl InstanceRuntime for VmRuntime {
    async fn start(&self, _: &StartRequest) -> Result<Started, RuntimeError> {
        Err(failed(
            "a virtual machine is started from its manifest, not from a container request",
        ))
    }

    async fn stop(&self, handle: &str) -> Result<(), RuntimeError> {
        if !valid_handle(handle) {
            return Ok(());
        }
        let dir = self.instance_dir(handle);
        if !dir.exists() {
            return Ok(());
        }
        let meta = std::fs::read_to_string(dir.join("meta.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Meta>(&t).ok());
        let tapped = meta.as_ref().is_some_and(|m| m.tap.is_some());
        let ports: Vec<u16> = meta
            .map(|m| m.ports.iter().map(|p| p.host).collect())
            .unwrap_or_default();
        if let Some(pid) = self.pid_of(&dir)
            && self.alive(handle, pid)
        {
            // Disposable: no graceful shutdown, the disk is thrown away anyway.
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
            for _ in 0..50 {
                if !self.alive(handle, pid) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        if tapped {
            self.drop_net(handle).await;
        }
        std::fs::remove_dir_all(&dir).map_err(|e| failed(format!("{}: {e}", dir.display())))?;
        self.release(&ports);
        Ok(())
    }

    async fn status(&self, handle: &str) -> Result<RuntimeStatus, RuntimeError> {
        if !valid_handle(handle) {
            return Ok(RuntimeStatus::Missing);
        }
        let dir = self.instance_dir(handle);
        if !dir.exists() {
            return Ok(RuntimeStatus::Missing);
        }
        Ok(match self.pid_of(&dir) {
            Some(pid) if self.alive(handle, pid) => RuntimeStatus::Running,
            _ => RuntimeStatus::Exited,
        })
    }

    async fn list_managed(&self) -> Result<Vec<Managed>, RuntimeError> {
        Ok(self
            .metas()
            .into_iter()
            .map(|m| Managed {
                handle: m.name,
                instance_id: m.instance_id,
            })
            .collect())
    }

    async fn ensure_lab(&self, _: &str, _: &str) -> Result<(), RuntimeError> {
        Err(failed(
            "private VPN networks are not available for virtual machines",
        ))
    }

    async fn remove_lab(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn reachable(&self, host_port: u16) -> bool {
        crate::client::probe(&format!("127.0.0.1:{host_port}")).await
    }
}

/// Which instances a metadata directory holds, for tests and `check`.
pub fn instance_names(rt: &VmRuntime) -> BTreeMap<String, String> {
    rt.metas()
        .into_iter()
        .map(|m| (m.name, m.instance_id))
        .collect()
}

/// Whether QEMU's `info usernet` table shows an established connection to the guest port.
pub fn usernet_established(table: &str, guest_port: u16) -> bool {
    table.lines().any(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // Protocol[State]  FD  Source Address  Port  Dest. Address  Port  RecvQ  SendQ
        cols.len() >= 6
            && cols[0] == "TCP[ESTABLISHED]"
            && cols[5].parse::<u16>().is_ok_and(|p| p == guest_port)
    })
}
