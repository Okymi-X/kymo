//! Challenge manifests: deployment data that lives on the lab host, not in the platform.
//!
//! One JSON file per artifact in the manifests directory:
//!
//! ```json
//! {
//!   "artifact": "localhost/kymo-lab/flag-server:1",
//!   "runtime": "docker",
//!   "port": 8000,
//!   "env": { "MODE": "ctf" },
//!   "env_allow": ["FLAG"],
//!   "cpu_millis": 500, "memory_mb": 256, "pids": 128, "max_ttl_secs": 3600
//! }
//! ```
//!
//! In manifest mode the lab host runs only artifacts it has a manifest for. The platform names
//! the artifact (the image reference configured on the challenge) and may ask for less than the
//! manifest allows; it cannot choose the image, the port or raise a limit, and it can set only
//! the environment variables the manifest lists in `env_allow`.

use std::collections::BTreeMap;
use std::path::Path;

use kymo_instances::{InstanceSpec, ResourceCaps};
use serde::{Deserialize, Serialize};

pub use crate::inject::{Inject, VerifierBlock};
pub use crate::protocol::PortDecl;

/// What a manifest may ask for: containers (`docker`, `podman`) and virtual machines (`vm`, run
/// with QEMU, KVM-accelerated when the host has it). Other runtimes plug in behind the same
/// `InstanceRuntime` interface.
pub const RUNTIMES: &[&str] = &["docker", "podman", "vm"];
pub const DISKS: &[&str] = &["sata", "ide", "virtio"];
pub const NICS: &[&str] = &["e1000", "virtio", "rtl8139"];
pub const FIRMWARES: &[&str] = &["bios", "uefi"];
const MAX_PORTS: usize = 8;
const DISK_EXTENSIONS: &[&str] = &[".ova", ".qcow2", ".vmdk", ".img", ".raw", ".vdi"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The reference the platform sends: normally the image tag.
    pub artifact: String,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    /// TCP port the service listens on inside the container. A virtual machine lists its services
    /// in `ports` instead.
    #[serde(default)]
    pub port: u16,
    /// A virtual machine's published services, primary first. The platform shows the first as the
    /// instance's address and the rest by name.
    #[serde(default)]
    pub ports: Vec<PortDecl>,
    /// A virtual machine's disk image (`.ova`, `.qcow2`, `.vmdk`, `.vdi`, `.img` or `.raw`), a file
    /// name inside the lab host's images directory.
    pub file: Option<String>,
    /// Seconds a virtual machine may take to come up before the start counts as failed.
    pub boot_timeout_secs: Option<u32>,
    /// Disk controller the guest sees: `sata` (default, works with nearly everything), `ide` or
    /// `virtio` (fastest, needs guest drivers).
    pub disk: Option<String>,
    /// Network card the guest sees: `e1000` (default), `virtio` or `rtl8139`.
    pub nic: Option<String>,
    /// `bios` (default, what VirtualBox exports use) or `uefi`.
    pub firmware: Option<String>,
    /// How players are told to reach it. `machine`: just its address, with every port open on it,
    /// to be scanned like a HackTheBox machine (the default for a virtual machine). `service`:
    /// the address and port of the service, like a HackTheBox challenge (the default for a
    /// container).
    pub display: Option<String>,
    /// Fixed environment, always set.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Variable names the platform may add or override.
    #[serde(default)]
    pub env_allow: Vec<String>,
    pub cpu_millis: Option<u32>,
    pub memory_mb: Option<u32>,
    pub pids: Option<u32>,
    pub max_ttl_secs: Option<i64>,
    pub read_only: Option<bool>,
    /// Where the flags the platform generates for a run are put. Without it the lab host refuses
    /// flags for this artifact.
    #[serde(default)]
    pub inject: Option<Inject>,
    /// A Verifier container started next to each instance, for a Responder challenge.
    #[serde(default)]
    pub verifier: Option<VerifierBlock>,
    /// Containers only. Capabilities added back after all are dropped, from a short allowlist
    /// (`kymo_instances::ALLOWED_CAPS`). For a challenge that needs a real login: sshd,
    /// sudo, a few services. Only the organizer can set it, here; the platform cannot ask.
    #[serde(default)]
    pub cap_add: Vec<String>,
    /// Containers only. `false` lets setuid programs such as `sudo` work. Default `true`.
    #[serde(default)]
    pub no_new_privileges: Option<bool>,
    /// Containers only. The container's host name, instead of Docker's random one.
    #[serde(default)]
    pub hostname: Option<String>,
}

fn default_runtime() -> String {
    "docker".into()
}

/// Why a request was refused. The text is shown to the platform's operator.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refusal(pub String);

impl Manifest {
    /// Checks the manifest itself: a usable artifact, a known runtime, a port.
    pub fn check(&self) -> Result<(), String> {
        if let Some(d) = &self.display
            && d != "machine"
            && d != "service"
        {
            return Err("display is machine or service".into());
        }
        if self.artifact.is_empty() || self.artifact.len() > 200 {
            return Err("artifact is missing or too long".into());
        }
        if !RUNTIMES.contains(&self.runtime.as_str()) {
            return Err(format!(
                "runtime {} is not supported here (use one of {})",
                self.runtime,
                RUNTIMES.join(", ")
            ));
        }
        if self.is_vm() {
            self.check_vm()?;
        } else {
            if self.port == 0 {
                return Err("port must be between 1 and 65535".into());
            }
            if !self.ports.is_empty() || self.file.is_some() {
                return Err(
                    "ports and file are for virtual machines (runtime vm); a container uses port"
                        .into(),
                );
            }
        }
        // Run the same validation the platform side uses, so a manifest can never describe
        // something the lab host would refuse to start.
        InstanceSpec {
            image: self.artifact.clone(),
            port: self.primary_port().max(1),
            cpu_millis: self.cpu_millis.unwrap_or(1),
            memory_mb: self.memory_mb.unwrap_or(1),
            pids: self.pids.unwrap_or(1),
            ttl_secs: self.max_ttl_secs.unwrap_or(1),
            env: self
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            read_only: true,
        }
        .validate(&ResourceCaps {
            max_cpu_millis: u32::MAX,
            max_memory_mb: u32::MAX,
            max_pids: u32::MAX,
            max_ttl_secs: i64::MAX,
        })
        .map_err(|e| e.to_string())?;
        for key in &self.env_allow {
            if key.is_empty()
                || !key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(format!("env_allow has a bad name: {key}"));
            }
        }
        if let Some(inject) = &self.inject {
            let reserved: Vec<String> = self
                .env
                .keys()
                .chain(self.env_allow.iter())
                .cloned()
                .collect();
            inject.check(self.is_vm(), &reserved)?;
        }
        if let Some(verifier) = &self.verifier {
            verifier.check()?;
        }
        if self.is_vm()
            && (!self.cap_add.is_empty()
                || self.no_new_privileges.is_some()
                || self.hostname.is_some())
        {
            return Err(
                "cap_add, no_new_privileges and hostname are for containers; a virtual machine already has its own kernel".into(),
            );
        }
        self.hardening().check()?;
        Ok(())
    }

    /// What this image relaxes of the container lock down. The default relaxes nothing.
    pub fn hardening(&self) -> kymo_instances::Hardening {
        kymo_instances::Hardening {
            cap_add: self.cap_add.clone(),
            no_new_privileges: self.no_new_privileges.unwrap_or(true),
            hostname: self.hostname.clone(),
        }
    }

    pub fn is_vm(&self) -> bool {
        self.runtime == "vm"
    }

    /// Whether players are given just an address (every port open on it) instead of the address
    /// of one service.
    pub fn machine_style(&self) -> bool {
        match self.display.as_deref() {
            Some(d) => d == "machine",
            None => self.is_vm(),
        }
    }

    /// The service the platform shows as the address: `port`, or a virtual machine's first port.
    pub fn primary_port(&self) -> u16 {
        self.ports.first().map_or(self.port, |p| p.port)
    }

    /// Every published service, primary first.
    pub fn declared_ports(&self) -> Vec<PortDecl> {
        if self.ports.is_empty() && self.port == 0 {
            // A machine on a lab network needs none: it has an address of its own.
            Vec::new()
        } else if self.ports.is_empty() {
            vec![PortDecl {
                name: "main".into(),
                port: self.port,
            }]
        } else {
            self.ports.clone()
        }
    }

    fn check_vm(&self) -> Result<(), String> {
        let file = self
            .file
            .as_deref()
            .ok_or("a virtual machine needs file: its image in the images directory")?;
        let lower = file.to_ascii_lowercase();
        if file.is_empty()
            || file.len() > 120
            || file.starts_with('.')
            || file.contains(['/', '\\', '\0'])
            || !file
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._- ()".contains(c))
            || !DISK_EXTENSIONS.iter().any(|e| lower.ends_with(e))
        {
            return Err(format!(
                "file must be a plain file name ending in {} (it is looked up in the images directory)",
                DISK_EXTENSIONS.join(", ")
            ));
        }
        // A machine shown as an address (the default) lists no ports: every port is open on its
        // own address. Shown as a service it must say which port to connect to.
        if self.ports.is_empty() && self.port == 0 && !self.machine_style() {
            return Err("a virtual machine shown as a service needs ports: the services players connect to (or leave display out, or set it to machine)".into());
        }
        if self.ports.len() > MAX_PORTS {
            return Err(format!("at most {MAX_PORTS} ports"));
        }
        let mut seen = std::collections::HashSet::new();
        for p in &self.ports {
            let ok_name = !p.name.is_empty()
                && p.name.len() <= 16
                && p.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if p.port == 0 || !ok_name {
                return Err(format!(
                    "port {} is not valid: a number and a lowercase name of up to 16 characters",
                    p.name
                ));
            }
            if !seen.insert(p.port) {
                return Err(format!("port {} is listed twice", p.port));
            }
        }
        for (field, value, allowed) in [
            ("disk", &self.disk, DISKS),
            ("nic", &self.nic, NICS),
            ("firmware", &self.firmware, FIRMWARES),
        ] {
            if let Some(v) = value
                && !allowed.contains(&v.as_str())
            {
                return Err(format!("{field} is one of {}", allowed.join(", ")));
            }
        }
        if self
            .boot_timeout_secs
            .is_some_and(|s| !(10..=1800).contains(&s))
        {
            return Err("boot_timeout_secs is between 10 and 1800".into());
        }
        if !self.env.is_empty() || !self.env_allow.is_empty() {
            return Err("environment variables cannot be set on a virtual machine".into());
        }
        Ok(())
    }

    /// The spec to run for a request: the manifest's image, port and fixed environment, the
    /// platform's own requests clamped to the manifest's and the lab host's limits.
    pub fn resolve(
        &self,
        asked: &InstanceSpec,
        caps: &ResourceCaps,
    ) -> Result<InstanceSpec, Refusal> {
        let clamp_u32 = |asked: u32, manifest: Option<u32>, cap: u32| {
            asked.min(manifest.unwrap_or(cap)).min(cap).max(1)
        };
        let ttl = asked
            .ttl_secs
            .min(self.max_ttl_secs.unwrap_or(caps.max_ttl_secs))
            .min(caps.max_ttl_secs)
            .max(1);
        if self.is_vm() && !asked.env.is_empty() {
            return Err(Refusal(
                "environment variables cannot be set on a virtual machine".into(),
            ));
        }
        let mut env: BTreeMap<String, String> = self.env.clone();
        for (key, value) in &asked.env {
            if !self.env_allow.contains(key) {
                return Err(Refusal(format!(
                    "the manifest for {} does not allow the platform to set {key}",
                    self.artifact
                )));
            }
            env.insert(key.clone(), value.clone());
        }
        let spec = InstanceSpec {
            image: self.artifact.clone(),
            // A machine with no ports has no service port; the spec still needs a valid one.
            port: self.primary_port().max(1),
            cpu_millis: clamp_u32(asked.cpu_millis, self.cpu_millis, caps.max_cpu_millis),
            memory_mb: clamp_u32(asked.memory_mb, self.memory_mb, caps.max_memory_mb),
            pids: clamp_u32(asked.pids, self.pids, caps.max_pids),
            ttl_secs: ttl,
            env: env.into_iter().collect(),
            // Read-only unless the manifest says otherwise; the platform cannot make it writable.
            read_only: self.read_only.unwrap_or(true),
        };
        spec.validate(caps).map_err(|e| Refusal(e.to_string()))?;
        Ok(spec)
    }
}

/// Reads every `*.json` file in a directory. A directory that does not exist is an empty set;
/// a file that does not parse or does not check is an error naming the file, so a typo is found
/// when the lab host starts and not when a player starts an instance.
pub fn load_dir(dir: &Path) -> Result<Vec<Manifest>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut out: Vec<Manifest> = Vec::new();
    for path in paths {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        manifest
            .check()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if out.iter().any(|m| m.artifact == manifest.artifact) {
            return Err(format!(
                "{}: artifact {} is listed twice",
                path.display(),
                manifest.artifact
            ));
        }
        out.push(manifest);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        serde_json::from_str(
            r#"{"artifact":"lab/web:1","port":8000,"env":{"MODE":"ctf"},"env_allow":["FLAG"],
                "cpu_millis":500,"memory_mb":256,"pids":128,"max_ttl_secs":1800}"#,
        )
        .unwrap()
    }

    fn asked(env: Vec<(&str, &str)>) -> InstanceSpec {
        InstanceSpec {
            image: "lab/web:1".into(),
            port: 9999,
            cpu_millis: 4000,
            memory_mb: 4096,
            pids: 900,
            ttl_secs: 7200,
            env: env.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
            read_only: false,
        }
    }

    #[test]
    fn a_container_manifest_can_relax_the_lock_down_within_the_allowlist() {
        let relaxed: Manifest = serde_json::from_str(
            r#"{"artifact":"lab/login:1","port":22,"read_only":false,
                "cap_add":["SETUID","SETGID","SYS_CHROOT"],"no_new_privileges":false}"#,
        )
        .unwrap();
        assert!(relaxed.check().is_ok());
        let h = relaxed.hardening();
        assert_eq!(h.cap_add, vec!["SETUID", "SETGID", "SYS_CHROOT"]);
        assert!(!h.no_new_privileges);
        // The platform still cannot change any of it: the spec resolved for a start carries only
        // limits, and the lock down is read from the manifest by the runtime.
        assert!(
            relaxed
                .resolve(&asked(vec![]), &ResourceCaps::default())
                .is_ok()
        );
    }

    #[test]
    fn a_container_can_name_its_host_and_a_machine_cannot() {
        let named: Manifest = serde_json::from_str(
            r#"{"artifact":"lab/box:1","port":80,"hostname":"atlas-intranet"}"#,
        )
        .unwrap();
        assert!(named.check().is_ok());
        assert_eq!(
            named.hardening().hostname.as_deref(),
            Some("atlas-intranet")
        );
        assert!(!named.hardening().is_relaxed(), "a name relaxes nothing");
        let bad: Manifest =
            serde_json::from_str(r#"{"artifact":"lab/box:1","port":80,"hostname":"Not Valid"}"#)
                .unwrap();
        assert!(bad.check().is_err());
        let vm: Manifest = serde_json::from_str(
            r#"{"artifact":"box","runtime":"vm","file":"b.qcow2","ports":[{"name":"ssh","port":22}],"hostname":"x"}"#,
        )
        .unwrap();
        assert!(vm.check().is_err());
    }

    #[test]
    fn the_default_manifest_relaxes_nothing() {
        let h = manifest().hardening();
        assert!(h.cap_add.is_empty() && h.no_new_privileges && !h.is_relaxed());
    }

    #[test]
    fn dangerous_capabilities_and_machines_are_refused() {
        for cap in ["SYS_ADMIN", "NET_ADMIN", "SYS_PTRACE", "ALL", "CAP_SETUID"] {
            let m: Manifest = serde_json::from_str(&format!(
                r#"{{"artifact":"lab/login:1","port":22,"cap_add":["{cap}"]}}"#
            ))
            .unwrap();
            assert!(m.check().is_err(), "{cap}");
        }
        // A virtual machine has its own kernel; the container settings make no sense there.
        let vm: Manifest = serde_json::from_str(
            r#"{"artifact":"box","runtime":"vm","file":"box.qcow2","ports":[{"name":"ssh","port":22}],"cap_add":["SETUID"]}"#,
        )
        .unwrap();
        assert!(vm.check().unwrap_err().contains("containers"));
    }

    #[test]
    fn the_manifest_decides_port_and_limits_and_the_platform_can_only_ask_for_less() {
        let spec = manifest()
            .resolve(&asked(vec![("FLAG", "KYMO{x}")]), &ResourceCaps::default())
            .unwrap();
        assert_eq!(spec.port, 8000, "the platform cannot pick the port");
        assert_eq!(
            (spec.cpu_millis, spec.memory_mb, spec.pids, spec.ttl_secs),
            (500, 256, 128, 1800)
        );
        assert_eq!(spec.env.len(), 2);
        assert!(spec.env.contains(&("MODE".into(), "ctf".into())));
        let small = InstanceSpec {
            memory_mb: 64,
            ttl_secs: 60,
            ..asked(vec![])
        };
        let spec = manifest()
            .resolve(&small, &ResourceCaps::default())
            .unwrap();
        assert_eq!(
            (spec.memory_mb, spec.ttl_secs),
            (64, 60),
            "asking for less is honoured"
        );
    }

    #[test]
    fn lab_wide_caps_apply_on_top_of_the_manifest() {
        let caps = ResourceCaps {
            max_memory_mb: 128,
            max_ttl_secs: 600,
            ..ResourceCaps::default()
        };
        let spec = manifest().resolve(&asked(vec![]), &caps).unwrap();
        assert_eq!((spec.memory_mb, spec.ttl_secs), (128, 600));
    }

    #[test]
    fn environment_outside_the_allow_list_is_refused() {
        let err = manifest()
            .resolve(&asked(vec![("SECRET", "x")]), &ResourceCaps::default())
            .unwrap_err();
        assert!(err.0.contains("SECRET"), "{err}");
        // A fixed variable cannot be overridden unless it is also allowed.
        assert!(
            manifest()
                .resolve(&asked(vec![("MODE", "evil")]), &ResourceCaps::default())
                .is_err()
        );
    }

    #[test]
    fn manifests_are_checked_when_loaded() {
        let bad = |json: &str| {
            serde_json::from_str::<Manifest>(json)
                .unwrap()
                .check()
                .is_err()
        };
        assert!(bad(r#"{"artifact":"","port":80}"#));
        assert!(bad(r#"{"artifact":"a/b:1","port":0}"#));
        assert!(bad(r#"{"artifact":"a/b:1","port":80,"runtime":"qemu"}"#));
        assert!(bad(r#"{"artifact":"-rm","port":80}"#));
        assert!(bad(
            r#"{"artifact":"a/b:1","port":80,"env_allow":["lower"]}"#
        ));
        assert!(!bad(r#"{"artifact":"a/b:1","port":80}"#));
    }

    #[test]
    fn a_directory_of_manifests_loads_and_reports_the_bad_file() {
        let dir = std::env::temp_dir().join(format!("kymo-lab-manifests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.json"), r#"{"artifact":"a/one:1","port":80}"#).unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        assert_eq!(load_dir(&dir).unwrap().len(), 1);
        std::fs::write(dir.join("b.json"), r#"{"artifact":"a/two:1","port":0}"#).unwrap();
        let err = load_dir(&dir).unwrap_err();
        assert!(err.contains("b.json"), "{err}");
        std::fs::write(dir.join("b.json"), r#"{"artifact":"a/one:1","port":81}"#).unwrap();
        assert!(load_dir(&dir).unwrap_err().contains("twice"));
        assert!(load_dir(&dir.join("missing")).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
