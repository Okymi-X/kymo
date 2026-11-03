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

/// What the lab host can run instances with. Only containers exist today; virtual machines and
/// other runtimes plug in behind the same `InstanceRuntime` interface.
pub const RUNTIMES: &[&str] = &["docker", "podman"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The reference the platform sends: normally the image tag.
    pub artifact: String,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    /// TCP port the service listens on inside the container.
    pub port: u16,
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
        if self.port == 0 {
            return Err("port must be between 1 and 65535".into());
        }
        // Run the same validation the platform side uses, so a manifest can never describe
        // something the lab host would refuse to start.
        InstanceSpec {
            image: self.artifact.clone(),
            port: self.port,
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
            port: self.port,
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
