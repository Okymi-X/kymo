//! Flag delivery on the lab host: what a manifest declares (`inject`, `verifier`), and the plan
//! that turns the flags of one run into environment variables, files to write over SSH, and
//! flags to hold for the Verifier. Pure: nothing here touches the network or the disk.

use kymo_instances::{FlagDelivery, RunFlag};
use serde::{Deserialize, Serialize};

use crate::manifest::Refusal;
use crate::protocol::LabPlacement;

const MAX_NAME: usize = 32;
const MAX_PATH: usize = 200;
const DEFAULT_SSH_PORT: u16 = 22;
const DEFAULT_VERIFIER_PORT: u16 = 8080;
const DEFAULT_VERIFIER_MEMORY_MB: u32 = 128;
const DEFAULT_VERIFIER_CPU_MILLIS: u32 = 250;

fn ssh_port() -> u16 {
    DEFAULT_SSH_PORT
}

fn verifier_port() -> u16 {
    DEFAULT_VERIFIER_PORT
}

/// How the lab host puts a run's flags into an instance. Containers get an environment
/// variable, virtual machines get a file written over SSH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inject {
    /// A virtual machine's SSH port.
    #[serde(default = "ssh_port")]
    pub port: u16,
    /// The account the files are written as (a virtual machine).
    pub user: Option<String>,
    /// Path to a private key. Preferred over the password when both are declared.
    pub ssh_key: Option<String>,
    pub ssh_password: Option<String>,
    /// Write through `sudo -n`.
    #[serde(default)]
    pub sudo: bool,
    pub flags: Vec<FlagTarget>,
}

/// Where one named flag goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagTarget {
    pub name: String,
    /// A container: the environment variable that carries it.
    pub env: Option<String>,
    /// A virtual machine: the absolute path of the file.
    pub path: Option<String>,
    /// `user` or `user:group`, applied after the file is written.
    pub owner: Option<String>,
    /// Octal permissions, for example `0640`. Defaults to `0600`.
    pub mode: Option<String>,
}

/// A Verifier the lab host starts next to each instance of this artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierBlock {
    /// A directory name under the verifiers directory, holding `verifier.json` and the scripts.
    pub definition: String,
    #[serde(default = "verifier_port")]
    pub port: u16,
    pub memory_mb: Option<u32>,
    pub cpu_millis: Option<u32>,
}

impl VerifierBlock {
    pub fn memory_mb(&self) -> u32 {
        self.memory_mb.unwrap_or(DEFAULT_VERIFIER_MEMORY_MB)
    }

    pub fn cpu_millis(&self) -> u32 {
        self.cpu_millis.unwrap_or(DEFAULT_VERIFIER_CPU_MILLIS)
    }

    pub fn check(&self) -> Result<(), String> {
        let name_ok = !self.definition.is_empty()
            && self.definition.len() <= 64
            && !self.definition.starts_with('.')
            && self
                .definition
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
        if !name_ok {
            return Err(
                "verifier.definition is a directory name: lowercase letters, digits, . _ -".into(),
            );
        }
        if self.port == 0 {
            return Err("verifier.port must be between 1 and 65535".into());
        }
        if !(16..=2048).contains(&self.memory_mb()) {
            return Err("verifier.memory_mb is 16 to 2048".into());
        }
        if !(50..=4000).contains(&self.cpu_millis()) {
            return Err("verifier.cpu_millis is 50 to 4000".into());
        }
        Ok(())
    }
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn env_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// A path that is safe to hand to a shell as one quoted word and cannot climb out of where it
/// was meant to go.
fn path_ok(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= MAX_PATH
        && !path.ends_with('/')
        && path.split('/').all(|part| part != "." && part != "..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
}

fn owner_ok(owner: &str) -> bool {
    let part_ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 32
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    };
    match owner.split_once(':') {
        Some((user, group)) => part_ok(user) && part_ok(group),
        None => part_ok(owner),
    }
}

fn mode_ok(mode: &str) -> bool {
    (3..=4).contains(&mode.len()) && mode.chars().all(|c| ('0'..='7').contains(&c))
}

impl Inject {
    /// Checks the block for a container (`vm` false) or a virtual machine. `reserved` are the
    /// environment names the manifest already uses.
    pub fn check(&self, vm: bool, reserved: &[String]) -> Result<(), String> {
        if self.flags.is_empty() {
            return Err("inject lists no flags".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for flag in &self.flags {
            if !name_ok(&flag.name) {
                return Err(format!(
                    "inject flag name {:?} is 1 to 32 characters: lowercase letters, digits, underscores",
                    flag.name
                ));
            }
            if !names.insert(flag.name.as_str()) {
                return Err(format!("inject names {} twice", flag.name));
            }
            if vm {
                if flag.env.is_some() {
                    return Err(format!(
                        "flag {}: a virtual machine takes a path, not an env variable",
                        flag.name
                    ));
                }
                let Some(path) = &flag.path else {
                    return Err(format!("flag {} needs a path", flag.name));
                };
                if !path_ok(path) {
                    return Err(format!(
                        "flag {}: the path is absolute, without .. and with only letters, digits and / . _ -",
                        flag.name
                    ));
                }
                if flag.owner.as_deref().is_some_and(|o| !owner_ok(o)) {
                    return Err(format!("flag {}: owner is user or user:group", flag.name));
                }
                if flag.mode.as_deref().is_some_and(|m| !mode_ok(m)) {
                    return Err(format!("flag {}: mode is octal, like 0640", flag.name));
                }
            } else {
                if flag.path.is_some() || flag.owner.is_some() || flag.mode.is_some() {
                    return Err(format!(
                        "flag {}: a container takes an env variable; paths are for virtual machines",
                        flag.name
                    ));
                }
                let Some(env) = &flag.env else {
                    return Err(format!("flag {} needs an env variable", flag.name));
                };
                if !env_name_ok(env) {
                    return Err(format!(
                        "flag {}: {env} is not a valid variable name",
                        flag.name
                    ));
                }
                if reserved.contains(env) {
                    return Err(format!(
                        "flag {}: {env} is already set by the manifest",
                        flag.name
                    ));
                }
            }
        }
        if vm {
            if self.port == 0 {
                return Err("inject.port must be between 1 and 65535".into());
            }
            if self.user.as_deref().is_none_or(str::is_empty) {
                return Err("inject needs a user for a virtual machine".into());
            }
            if self.ssh_key.is_none() && self.ssh_password.is_none() {
                return Err("inject needs ssh_key or ssh_password".into());
            }
        } else if self.user.is_some() || self.ssh_key.is_some() || self.ssh_password.is_some() {
            return Err("ssh settings in inject are for virtual machines".into());
        }
        Ok(())
    }
}

/// A file to write into a virtual machine. The value is a secret and stays out of `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct FileWrite {
    pub path: String,
    pub owner: Option<String>,
    pub mode: String,
    pub value: String,
}

impl std::fmt::Debug for FileWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWrite")
            .field("path", &self.path)
            .field("owner", &self.owner)
            .field("mode", &self.mode)
            .field("value", &"<hidden>")
            .finish()
    }
}

/// What to do with the flags of one run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlagPlan {
    /// Environment variables for a container.
    pub env: Vec<(String, String)>,
    /// Files for a virtual machine.
    pub files: Vec<FileWrite>,
    /// Flags only the Verifier may release.
    pub held: Vec<RunFlag>,
    /// Whether a Verifier container starts with this run.
    pub verifier: bool,
}

fn refuse(message: impl Into<String>) -> Refusal {
    Refusal(message.into())
}

/// Turns the flags a run came with into a plan, refusing anything the manifest did not declare.
/// A request that names a flag the manifest has no place for is refused, so a typo cannot start
/// an instance without its flag. The Verifier and its address must agree on both sides.
pub fn plan_flags(
    flags: &[RunFlag],
    inject: Option<&Inject>,
    verifier: Option<&VerifierBlock>,
    vm: bool,
    lab: Option<&LabPlacement>,
) -> Result<FlagPlan, Refusal> {
    let wants_verifier = lab.is_some_and(|l| l.verifier_ip.is_some());
    if wants_verifier && verifier.is_none() {
        return Err(refuse("this image has no verifier in its manifest"));
    }
    if !wants_verifier && verifier.is_some() {
        return Err(refuse(
            "this image has a verifier, so the platform must give it an address",
        ));
    }
    let mut plan = FlagPlan {
        verifier: wants_verifier,
        ..FlagPlan::default()
    };
    for flag in flags {
        match flag.deliver {
            FlagDelivery::Hold => {
                if verifier.is_none() {
                    return Err(refuse("a held flag needs a verifier"));
                }
                plan.held.push(flag.clone());
            }
            FlagDelivery::Inject => {
                let target = inject
                    .and_then(|i| i.flags.iter().find(|t| t.name == flag.name))
                    .ok_or_else(|| {
                        refuse(format!(
                            "the manifest has no place for the flag named {}",
                            flag.name
                        ))
                    })?;
                if vm {
                    plan.files.push(FileWrite {
                        path: target
                            .path
                            .clone()
                            .ok_or_else(|| refuse("flag has no path"))?,
                        owner: target.owner.clone(),
                        mode: target.mode.clone().unwrap_or_else(|| "0600".into()),
                        value: flag.value.clone(),
                    });
                } else {
                    plan.env.push((
                        target
                            .env
                            .clone()
                            .ok_or_else(|| refuse("flag has no env"))?,
                        flag.value.clone(),
                    ));
                }
            }
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(name: &str, deliver: FlagDelivery) -> RunFlag {
        RunFlag {
            name: name.into(),
            value: format!("KYMO{{{name}}}"),
            deliver,
        }
    }

    fn container() -> Inject {
        serde_json::from_str(r#"{"flags":[{"name":"flag","env":"FLAG"}]}"#).unwrap()
    }

    fn machine() -> Inject {
        serde_json::from_str(
            r#"{"user":"root","ssh_key":"/etc/kymo-lab/keys/b2r.key","flags":[
                {"name":"user","path":"/home/ctf/user.txt","owner":"ctf:ctf","mode":"0640"},
                {"name":"root","path":"/root/root.txt"}]}"#,
        )
        .unwrap()
    }

    fn lab(verifier_ip: Option<&str>) -> LabPlacement {
        LabPlacement {
            network: "kymo-lab-1".into(),
            ip: "10.77.0.42".into(),
            bind: "10.77.0.33".into(),
            verifier_ip: verifier_ip.map(str::to_owned),
        }
    }

    #[test]
    fn a_container_flag_becomes_an_environment_variable() {
        let plan = plan_flags(
            &[flag("flag", FlagDelivery::Inject)],
            Some(&container()),
            None,
            false,
            None,
        )
        .unwrap();
        assert_eq!(plan.env, vec![("FLAG".to_owned(), "KYMO{flag}".to_owned())]);
        assert!(plan.files.is_empty() && plan.held.is_empty() && !plan.verifier);
    }

    #[test]
    fn a_machine_flag_becomes_a_file_with_the_declared_owner_and_mode() {
        let plan = plan_flags(
            &[
                flag("user", FlagDelivery::Inject),
                flag("root", FlagDelivery::Inject),
            ],
            Some(&machine()),
            None,
            true,
            None,
        )
        .unwrap();
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].path, "/home/ctf/user.txt");
        assert_eq!(plan.files[0].owner.as_deref(), Some("ctf:ctf"));
        assert_eq!(plan.files[0].mode, "0640");
        assert_eq!(plan.files[1].mode, "0600");
        assert!(!format!("{:?}", plan.files[0]).contains("KYMO{user}"));
    }

    #[test]
    fn a_flag_with_no_place_in_the_manifest_is_refused() {
        let err = plan_flags(
            &[flag("typo", FlagDelivery::Inject)],
            Some(&container()),
            None,
            false,
            None,
        )
        .unwrap_err();
        assert!(err.0.contains("typo"));
        assert!(
            plan_flags(
                &[flag("flag", FlagDelivery::Inject)],
                None,
                None,
                false,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn a_held_flag_needs_a_verifier_and_the_two_sides_must_agree_on_it() {
        let block = VerifierBlock {
            definition: "ghost-files".into(),
            port: 8080,
            memory_mb: None,
            cpu_millis: None,
        };
        let held = [flag("flag", FlagDelivery::Hold)];
        // The platform sent an address and the manifest has a verifier: fine.
        let plan = plan_flags(
            &held,
            None,
            Some(&block),
            true,
            Some(&lab(Some("10.77.0.43"))),
        )
        .unwrap();
        assert!(plan.verifier);
        assert_eq!(plan.held.len(), 1);
        // The manifest has a verifier but the platform sent no address, and the other way round.
        assert!(plan_flags(&held, None, Some(&block), true, Some(&lab(None))).is_err());
        assert!(plan_flags(&held, None, None, true, Some(&lab(Some("10.77.0.43")))).is_err());
        // A held flag with no verifier at all.
        assert!(plan_flags(&held, None, None, true, Some(&lab(None))).is_err());
    }

    #[test]
    fn an_ordinary_instance_has_an_empty_plan() {
        let plan = plan_flags(&[], None, None, false, None).unwrap();
        assert_eq!(plan, FlagPlan::default());
    }

    #[test]
    fn inject_blocks_are_checked() {
        assert!(container().check(false, &[]).is_ok());
        assert!(machine().check(true, &[]).is_ok());
        // A container with a path, a machine with an env variable, and mixed settings.
        let bad = |json: &str, vm: bool| {
            serde_json::from_str::<Inject>(json)
                .unwrap()
                .check(vm, &["MODE".to_owned()])
        };
        assert!(bad(r#"{"flags":[{"name":"f","path":"/x"}]}"#, false).is_err());
        assert!(
            bad(
                r#"{"flags":[{"name":"f","env":"FLAG"}],"user":"root"}"#,
                false
            )
            .is_err()
        );
        assert!(bad(r#"{"flags":[{"name":"f","env":"MODE"}]}"#, false).is_err());
        assert!(bad(r#"{"flags":[{"name":"f","env":"lower"}]}"#, false).is_err());
        assert!(bad(r#"{"flags":[]}"#, false).is_err());
        assert!(
            bad(
                r#"{"flags":[{"name":"f","env":"A"},{"name":"f","env":"B"}]}"#,
                false
            )
            .is_err()
        );
        let vm = |flag: &str, extra: &str| {
            let json = format!(r#"{{"user":"root","ssh_password":"x"{extra},"flags":[{flag}]}}"#);
            serde_json::from_str::<Inject>(&json)
                .unwrap()
                .check(true, &[])
        };
        assert!(vm(r#"{"name":"f","path":"/root/f.txt"}"#, "").is_ok());
        assert!(vm(r#"{"name":"f","path":"relative.txt"}"#, "").is_err());
        assert!(vm(r#"{"name":"f","path":"/a/../b"}"#, "").is_err());
        assert!(vm(r#"{"name":"f","path":"/a b"}"#, "").is_err());
        assert!(vm(r#"{"name":"f","path":"/a;rm"}"#, "").is_err());
        assert!(vm(r#"{"name":"f","path":"/a","mode":"9"}"#, "").is_err());
        assert!(vm(r#"{"name":"f","path":"/a","owner":"a b"}"#, "").is_err());
        assert!(vm(r#"{"name":"f","env":"X"}"#, "").is_err());
        // A machine needs a user and a credential.
        let no_cred: Inject =
            serde_json::from_str(r#"{"user":"root","flags":[{"name":"f","path":"/a"}]}"#).unwrap();
        assert!(no_cred.check(true, &[]).is_err());
        let no_user: Inject =
            serde_json::from_str(r#"{"ssh_password":"x","flags":[{"name":"f","path":"/a"}]}"#)
                .unwrap();
        assert!(no_user.check(true, &[]).is_err());
    }

    #[test]
    fn verifier_blocks_are_checked() {
        let ok: VerifierBlock = serde_json::from_str(r#"{"definition":"ghost-files"}"#).unwrap();
        assert_eq!(ok.port, 8080);
        assert!(ok.check().is_ok());
        for bad in [
            r#"{"definition":""}"#,
            r#"{"definition":"../x"}"#,
            r#"{"definition":".hidden"}"#,
            r#"{"definition":"a b"}"#,
            r#"{"definition":"x","port":0}"#,
            r#"{"definition":"x","memory_mb":4}"#,
            r#"{"definition":"x","cpu_millis":10}"#,
        ] {
            let block: VerifierBlock = serde_json::from_str(bad).unwrap();
            assert!(block.check().is_err(), "{bad}");
        }
    }
}
