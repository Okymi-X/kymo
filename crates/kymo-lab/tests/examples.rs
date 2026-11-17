//! The example manifests shipped with the lab host are valid, so an organizer who copies one
//! starts from something that loads.

use std::path::PathBuf;

use kymo_lab::manifest::Manifest;

fn examples() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../deploy/lab/manifests");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".json.example"))
        .collect();
    found.sort();
    found
}

#[test]
fn every_example_manifest_parses_and_checks() {
    let found = examples();
    assert!(found.len() >= 8, "{found:?}");
    for path in found {
        let text = std::fs::read_to_string(&path).unwrap();
        let manifest: Manifest =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        manifest
            .check()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn the_flag_examples_show_each_way_a_flag_reaches_an_instance() {
    let load = |name: &str| -> Manifest {
        let path = examples()
            .into_iter()
            .find(|p| p.to_string_lossy().ends_with(name))
            .unwrap();
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let container = load("dynamic-flag.json.example");
    assert_eq!(
        container.inject.unwrap().flags[0].env.as_deref(),
        Some("FLAG")
    );
    let machine = load("boot2root.json.example");
    let inject = machine.inject.unwrap();
    assert_eq!(inject.flags.len(), 2);
    assert!(inject.ssh_key.is_some() && inject.ssh_password.is_some());
    let responder = load("responder.json.example");
    assert!(responder.verifier.is_some() && responder.inject.is_none());
}

#[test]
fn the_login_container_example_relaxes_only_what_the_allowlist_permits() {
    let path = examples()
        .into_iter()
        .find(|p| {
            p.to_string_lossy()
                .ends_with("container-with-login.json.example")
        })
        .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    let manifest: Manifest = serde_json::from_str(&text).unwrap();
    let h = manifest.hardening();
    assert!(h.is_relaxed() && !h.no_new_privileges);
    assert!(h.check().is_ok());
    // The explanation is in the file, so an organizer who copies it sees why.
    assert!(text.contains("_comment_cap_add") && text.contains("_comment_no_new_privileges"));
}

#[test]
fn the_language_examples_declare_a_judge_and_the_instance_examples_do_not() {
    let judges: Vec<_> = examples()
        .into_iter()
        .filter(|p| p.to_string_lossy().contains("/judge-"))
        .collect();
    assert!(judges.len() >= 4, "{judges:?}");
    for path in &judges {
        let manifest: Manifest =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let judge = manifest
            .judge
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no judge block", path.display()));
        assert!(
            judge.label.is_some(),
            "{} should say what players see",
            path.display()
        );
        assert!(manifest.inject.is_none() && manifest.verifier.is_none());
    }
    for path in examples()
        .into_iter()
        .filter(|p| !p.to_string_lossy().contains("/judge-"))
    {
        let manifest: Manifest =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            manifest.judge.is_none(),
            "{} is not a language",
            path.display()
        );
    }
}

#[test]
fn every_manifest_field_is_shown_in_at_least_one_example() {
    // A new field that no example uses is easy to miss, so the examples must keep up with the
    // manifest. The field names are read from the struct's source.
    let source = include_str!("../src/manifest.rs");
    let start = source.find("pub struct Manifest").unwrap();
    let end = start + source[start..].find("\n}\n").unwrap();
    let fields: Vec<&str> = source[start..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub "))
        .filter(|l| !l.starts_with("struct"))
        .filter_map(|l| l.split(':').next())
        .collect();
    assert!(fields.len() > 15, "{fields:?}");
    let all: String = examples()
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    let missing: Vec<&&str> = fields
        .iter()
        .filter(|f| !all.contains(&format!("\"{f}\"")))
        .collect();
    assert!(missing.is_empty(), "no example shows: {missing:?}");
}

#[test]
fn every_example_says_what_it_is_for() {
    for path in examples() {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let has_comment = value
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("_comment"));
        // The judge samples are short on purpose: the file name and the label say it all.
        let judge = value.get("judge").is_some();
        assert!(has_comment || judge, "{} has no _comment", path.display());
    }
}
