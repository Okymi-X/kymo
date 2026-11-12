//! The template shipped with Kymo is a working starting point: it loads, its function names
//! exist, and its checks run (against a closed port they are broken, never a crash).

use std::path::PathBuf;

use kymo_verifier::{Context, State, load, run_all};

fn template_copy() -> tempfile::TempDir {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../deploy/lab/verifiers/template");
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, dir.path().join(path.file_name().unwrap())).unwrap();
        }
    }
    // The author supplies the key; the template only names where it goes.
    std::fs::create_dir_all(dir.path().join("keys")).unwrap();
    std::fs::write(dir.path().join("keys/verify.key"), "not a real key").unwrap();
    dir
}

#[test]
fn the_template_loads_with_its_checks_and_function_names() {
    let dir = template_copy();
    let loaded = load(dir.path()).unwrap_or_else(|p| panic!("{p:?}"));
    assert_eq!(loaded.definition.checks.len(), 2);
    assert!(loaded.definition.checks.iter().all(|c| c.health.is_some()));
}

#[tokio::test]
async fn the_bash_check_of_the_template_runs_and_reports_broken_when_nothing_answers() {
    let dir = template_copy();
    let loaded = load(dir.path()).unwrap();
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib");
    // A port nothing listens on: the health function fails, so the step is broken, not fixed.
    let ctx = Context::new(&loaded, "127.0.0.1:1", vec![], &lib).unwrap();
    let out = run_all(&loaded, &ctx).await;
    assert_eq!(out[0].state, State::Broken, "{}", out[0].detail);
}

#[test]
fn the_schema_is_valid_json_and_names_every_field_the_code_reads() {
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema/verifier.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&text).unwrap();
    let props = schema["properties"].as_object().unwrap();
    for field in [
        "version",
        "title",
        "min_interval_secs",
        "run_timeout_secs",
        "target",
        "checks",
        "messages",
    ] {
        assert!(props.contains_key(field), "{field}");
    }
    let check = schema["properties"]["checks"]["items"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "id",
        "title",
        "language",
        "file",
        "function",
        "health",
        "timeout_secs",
    ] {
        assert!(check.contains_key(field), "{field}");
    }
}
