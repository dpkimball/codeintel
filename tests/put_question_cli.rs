//! CLI integration: `put-question` writes JSON and persists to bindb.

use std::process::Command;

#[test]
fn put_question_cli_writes_stored_json() {
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path();
    let codeintel = std::env::var("CARGO_BIN_EXE_codeintel")
        .map(std::path::PathBuf::from)
        .expect("CARGO_BIN_EXE_codeintel must be set when integration tests run");

    let put = Command::new(&codeintel)
        .args([
            "put-question",
            "--store",
            &store.display().to_string(),
            "--repo",
            "codeintel",
            "--id",
            "CLI-T1",
            "--question",
            "What is the write boundary?",
            "--answer",
            "codeintel writes bindb; binder-ai reads via CLI only.",
        ])
        .output()
        .unwrap();
    assert!(
        put.status.success(),
        "put-question failed: {}",
        String::from_utf8_lossy(&put.stderr)
    );

    let value: serde_json::Value = serde_json::from_slice(&put.stdout).unwrap();
    assert_eq!(value["stored"], true);
    assert_eq!(value["id"], "CLI-T1");
    assert_eq!(value["repo"], "codeintel");
}
