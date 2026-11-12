//! The Verifier definition: a `verifier.json` and the scripts it names, written by the
//! organizer for one challenge. This module reads it and checks it, so a mistake is found when
//! the lab host starts or when `kymo-lab check` runs, not when a player presses Verify.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_CHECKS: usize = 20;
const MAX_ID: usize = 32;
const MAX_TITLE: usize = 80;
const MAX_TIMEOUT_SECS: u64 = 60;
const MAX_RUN_TIMEOUT_SECS: u64 = 300;
const MAX_MESSAGE: usize = 200;

fn default_interval() -> u64 {
    20
}

fn default_check_timeout() -> u64 {
    10
}

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Bash,
    Python,
    /// An executable file in the definition directory, in any language. It is run as
    /// `./file <function>`.
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub version: u32,
    /// Shown at the top of the Verifier page.
    #[serde(default)]
    pub title: Option<String>,
    /// Least time between two Verify runs of one instance.
    #[serde(default = "default_interval")]
    pub min_interval_secs: u64,
    /// Cap for one whole run. Defaults to the sum of the check timeouts (and their health
    /// functions'), at most 300 seconds.
    #[serde(default)]
    pub run_timeout_secs: Option<u64>,
    #[serde(default)]
    pub target: Option<Target>,
    pub checks: Vec<Check>,
    #[serde(default)]
    pub messages: Option<Messages>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    #[serde(default)]
    pub ssh: Option<Ssh>,
}

/// How the scripts reach the target over SSH. Both secrets are optional; when both are given
/// the key is tried first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ssh {
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    /// A private key file, relative to the definition directory.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub sudo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub id: String,
    /// Shown to the player. The author chooses it so that it does not give the weakness away.
    pub title: String,
    pub language: Language,
    pub file: String,
    pub function: String,
    /// A second function in the same file, run first. If it does not succeed the check is
    /// Broken and the main function is not run.
    #[serde(default)]
    pub health: Option<String>,
    #[serde(default = "default_check_timeout")]
    pub timeout_secs: u64,
}

/// Optional replacements for the three messages players see.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Messages {
    pub fixed: Option<String>,
    pub failed: Option<String>,
    pub broken: Option<String>,
}

/// A definition and where it was read from.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub dir: PathBuf,
    pub definition: Definition,
}

impl Definition {
    pub fn run_timeout_secs(&self) -> u64 {
        self.run_timeout_secs.unwrap_or_else(|| {
            let sum: u64 = self
                .checks
                .iter()
                .map(|c| c.timeout_secs * if c.health.is_some() { 2 } else { 1 })
                .sum();
            sum.min(MAX_RUN_TIMEOUT_SECS)
        })
    }

    pub fn message_fixed(&self) -> &str {
        self.text(|m| m.fixed.as_deref(), "Step fixed")
    }

    pub fn message_failed(&self) -> &str {
        self.text(|m| m.failed.as_deref(), "Weakness still present")
    }

    pub fn message_broken(&self) -> &str {
        self.text(
            |m| m.broken.as_deref(),
            "Something this step depends on is broken. Restore it, or restart the instance",
        )
    }

    fn text<'a>(
        &'a self,
        pick: impl Fn(&'a Messages) -> Option<&'a str>,
        default: &'a str,
    ) -> &'a str {
        self.messages.as_ref().and_then(pick).unwrap_or(default)
    }

    /// The rules that need no files: shapes, lengths and uniqueness. Returns every problem, so
    /// an author fixes them in one pass.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.version != FORMAT_VERSION {
            out.push(format!("version must be {FORMAT_VERSION}"));
        }
        if self.checks.is_empty() || self.checks.len() > MAX_CHECKS {
            out.push(format!("checks has 1 to {MAX_CHECKS} entries"));
        }
        if !(1..=3600).contains(&self.min_interval_secs) {
            out.push("min_interval_secs is 1 to 3600".into());
        }
        if self
            .run_timeout_secs
            .is_some_and(|t| !(1..=MAX_RUN_TIMEOUT_SECS).contains(&t))
        {
            out.push(format!("run_timeout_secs is 1 to {MAX_RUN_TIMEOUT_SECS}"));
        }
        if let Some(title) = &self.title
            && title.chars().count() > MAX_TITLE
        {
            out.push("title is at most 80 characters".into());
        }
        if let Some(ssh) = self.target.as_ref().and_then(|t| t.ssh.as_ref()) {
            if ssh.user.is_empty() || ssh.user.len() > 64 {
                out.push("target.ssh.user is 1 to 64 characters".into());
            }
            if ssh.port == 0 {
                out.push("target.ssh.port must be between 1 and 65535".into());
            }
            if let Some(key) = &ssh.key
                && !safe_relative(key)
            {
                out.push("target.ssh.key is a path inside the definition directory".into());
            }
        }
        if let Some(m) = &self.messages {
            for (name, text) in [
                ("fixed", &m.fixed),
                ("failed", &m.failed),
                ("broken", &m.broken),
            ] {
                if text
                    .as_ref()
                    .is_some_and(|t| t.is_empty() || t.chars().count() > MAX_MESSAGE)
                {
                    out.push(format!("messages.{name} is 1 to {MAX_MESSAGE} characters"));
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for check in &self.checks {
            let at = |what: &str| format!("check {:?}: {what}", check.id);
            if !id_ok(&check.id) {
                out.push(at("id is 1 to 32 characters of a-z, 0-9, - and _"));
            }
            if !seen.insert(check.id.as_str()) {
                out.push(at("id is used twice"));
            }
            if check.title.trim().is_empty() || check.title.chars().count() > MAX_TITLE {
                out.push(at("title is 1 to 80 characters"));
            }
            if !safe_relative(&check.file) {
                out.push(at(
                    "file is a path inside the definition directory, without ..",
                ));
            }
            if !function_ok(&check.function) {
                out.push(at("function is a name of letters, digits and _"));
            }
            if check.health.as_deref().is_some_and(|h| !function_ok(h)) {
                out.push(at("health is a name of letters, digits and _"));
            }
            if !(1..=MAX_TIMEOUT_SECS).contains(&check.timeout_secs) {
                out.push(at("timeout_secs is 1 to 60"));
            }
        }
        out
    }
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn function_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A relative path that stays inside the directory: no leading slash, no `..`, no odd bytes.
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
}

/// Reads `verifier.json` from a definition directory and checks it, files included. Returns
/// every problem found.
pub fn load(dir: &Path) -> Result<Loaded, Vec<String>> {
    let path = dir.join("verifier.json");
    let text =
        std::fs::read_to_string(&path).map_err(|e| vec![format!("{}: {e}", path.display())])?;
    let definition: Definition =
        serde_json::from_str(&text).map_err(|e| vec![format!("{}: {e}", path.display())])?;
    let mut problems = definition.problems();
    for check in &definition.checks {
        let at = |what: String| format!("check {:?}: {what}", check.id);
        let file = dir.join(&check.file);
        match std::fs::read_to_string(&file) {
            Ok(body) => {
                for name in std::iter::once(&check.function).chain(check.health.iter()) {
                    if !defines(check.language, &body, name) && check.language != Language::Exec {
                        problems.push(at(format!("{} does not define {name}", check.file)));
                    }
                }
            }
            // An executable may be a binary: only its presence matters.
            Err(_) if check.language == Language::Exec && file.is_file() => {}
            Err(e) => problems.push(at(format!("cannot read {}: {e}", check.file))),
        }
    }
    if let Some(key) = definition
        .target
        .as_ref()
        .and_then(|t| t.ssh.as_ref())
        .and_then(|s| s.key.as_ref())
        && std::fs::metadata(dir.join(key)).is_err()
    {
        problems.push(format!("target.ssh.key {key} cannot be read"));
    }
    if problems.is_empty() {
        Ok(Loaded {
            dir: dir.to_owned(),
            definition,
        })
    } else {
        Err(problems)
    }
}

/// Whether a script defines a function of that name. A plain text scan, enough to catch a typo
/// in `verifier.json` before the event.
fn defines(language: Language, body: &str, name: &str) -> bool {
    body.lines()
        .map(str::trim_start)
        .any(|line| match language {
            Language::Bash => {
                line.strip_prefix(name)
                    .is_some_and(|rest| rest.trim_start().starts_with("()"))
                    || line
                        .strip_prefix("function ")
                        .and_then(|rest| rest.trim_start().strip_prefix(name))
                        .is_some_and(|after| {
                            !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                        })
            }
            Language::Python => line
                .strip_prefix("def ")
                .is_some_and(|rest| rest.trim_start().starts_with(&format!("{name}("))),
            Language::Exec => true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "version": 1, "title": "Atlas",
        "target": { "ssh": { "user": "verify", "key": "keys/verify.key" } },
        "checks": [
          { "id": "archive", "title": "Hidden in plain view", "language": "bash",
            "file": "checks.sh", "function": "check_archive", "health": "health_archive" },
          { "id": "credential", "title": "Past its expiry", "language": "python",
            "file": "checks.py", "function": "check_credential" }
        ] }"#;

    fn parse(json: &str) -> Definition {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_good_definition_has_no_problems_and_sensible_defaults() {
        let d = parse(GOOD);
        assert!(d.problems().is_empty(), "{:?}", d.problems());
        assert_eq!(d.min_interval_secs, 20);
        assert_eq!(d.checks[0].timeout_secs, 10);
        // Two checks of 10 seconds, the first with a health function: 30 seconds in all.
        assert_eq!(d.run_timeout_secs(), 30);
        assert_eq!(d.message_fixed(), "Step fixed");
    }

    #[test]
    fn every_rule_names_its_own_problem() {
        let bad = |edit: &dyn Fn(&mut Definition)| {
            let mut d = parse(GOOD);
            edit(&mut d);
            d.problems()
        };
        assert!(!bad(&|d| d.version = 2).is_empty());
        assert!(!bad(&|d| d.checks.clear()).is_empty());
        assert!(!bad(&|d| d.checks[0].id = "Bad Id".into()).is_empty());
        assert!(!bad(&|d| d.checks[1].id = "archive".into()).is_empty());
        assert!(!bad(&|d| d.checks[0].title = " ".into()).is_empty());
        assert!(!bad(&|d| d.checks[0].file = "../x.sh".into()).is_empty());
        assert!(!bad(&|d| d.checks[0].file = "/etc/passwd".into()).is_empty());
        assert!(!bad(&|d| d.checks[0].function = "1bad".into()).is_empty());
        assert!(!bad(&|d| d.checks[0].function = "a b".into()).is_empty());
        assert!(!bad(&|d| d.checks[0].health = Some("x;y".into())).is_empty());
        assert!(!bad(&|d| d.checks[0].timeout_secs = 0).is_empty());
        assert!(!bad(&|d| d.checks[0].timeout_secs = 61).is_empty());
        assert!(!bad(&|d| d.min_interval_secs = 0).is_empty());
        assert!(!bad(&|d| d.run_timeout_secs = Some(301)).is_empty());
        assert!(
            !bad(&|d| d
                .target
                .as_mut()
                .unwrap()
                .ssh
                .as_mut()
                .unwrap()
                .user
                .clear())
            .is_empty()
        );
        assert!(
            !bad(&|d| d.target.as_mut().unwrap().ssh.as_mut().unwrap().key = Some("/abs".into()))
                .is_empty()
        );
        let many: Vec<Check> = (0..21)
            .map(|i| Check {
                id: format!("c{i}"),
                ..parse(GOOD).checks[0].clone()
            })
            .collect();
        assert!(!bad(&|d| d.checks = many.clone()).is_empty());
    }

    #[test]
    fn unknown_languages_and_fields_are_rejected_when_parsing() {
        assert!(serde_json::from_str::<Definition>(&GOOD.replace("\"bash\"", "\"perl\"")).is_err());
    }

    #[test]
    fn messages_can_be_replaced_but_not_emptied() {
        let mut d = parse(GOOD);
        d.messages = Some(Messages {
            fixed: Some("Done".into()),
            failed: None,
            broken: Some(String::new()),
        });
        assert_eq!(d.message_fixed(), "Done");
        assert_eq!(d.message_failed(), "Weakness still present");
        assert!(d.problems().iter().any(|p| p.contains("messages.broken")));
    }

    #[test]
    fn loading_checks_the_files_and_the_function_names() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        };
        write("verifier.json", GOOD);
        // Nothing else exists yet: every problem is listed at once.
        let problems = load(dir.path()).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("checks.sh")));
        assert!(problems.iter().any(|p| p.contains("checks.py")));
        assert!(problems.iter().any(|p| p.contains("keys/verify.key")));
        write(
            "checks.sh",
            "check_archive() {\n  fixed\n}\nhealth_archive() { true; }\n",
        );
        write("checks.py", "def check_credential():\n    pass\n");
        write("keys/verify.key", "key");
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded.definition.checks.len(), 2);
        // A function that is not in the file is found before the event.
        write("checks.sh", "check_archive() { :; }\n");
        let problems = load(dir.path()).unwrap_err();
        assert!(
            problems.iter().any(|p| p.contains("health_archive")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_missing_or_broken_verifier_json_is_one_clear_problem() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).unwrap_err()[0].contains("verifier.json"));
        std::fs::write(dir.path().join("verifier.json"), "{ not json").unwrap();
        assert!(load(dir.path()).unwrap_err()[0].contains("verifier.json"));
    }

    #[test]
    fn the_function_scan_understands_both_bash_styles_and_python() {
        assert!(defines(Language::Bash, "foo() { :; }", "foo"));
        assert!(defines(Language::Bash, "  foo ()\n{ :; }", "foo"));
        assert!(defines(Language::Bash, "function foo { :; }", "foo"));
        assert!(!defines(Language::Bash, "foobar() { :; }", "foo"));
        assert!(!defines(Language::Bash, "function foobar { :; }", "foo"));
        assert!(defines(Language::Python, "def foo():\n  pass", "foo"));
        assert!(!defines(Language::Python, "def foobar():\n  pass", "foo"));
    }
}
