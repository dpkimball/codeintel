use codeintel::cargo_graph::scan_workspace;
use codeintel::indexer::{index, IndexOptions};
use codeintel::query::QueryEngine;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?} failed");
}

fn write_fixture(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
members = ["alpha", "beta"]
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("alpha/src")).unwrap();
    fs::write(
        root.join("alpha/Cargo.toml"),
        r#"
[package]
name = "alpha"
version = "0.1.0"
edition = "2021"
[dependencies]
beta = { path = "../beta" }
"#,
    )
    .unwrap();
    fs::write(
        root.join("alpha/src/lib.rs"),
        r#"
pub fn used() {}

pub fn unused_public() {}

fn private_calls() {
    used();
}

pub fn clone_body() {
    let a = 1;
    let b = a + 2;
    let c = b * 3;
    let _ = c;
}

pub trait Marker {}
pub struct Widget;
impl Marker for Widget {}
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("beta/src")).unwrap();
    fs::write(
        root.join("beta/Cargo.toml"),
        r#"
[package]
name = "beta"
version = "0.1.0"
edition = "2021"
[dependencies]
alpha = { path = "../alpha" }
"#,
    )
    .unwrap();
    fs::write(
        root.join("beta/src/lib.rs"),
        r#"
pub fn clone_body() {
    let a = 1;
    let b = a + 2;
    let c = b * 3;
    let _ = c;
}
"#,
    )
    .unwrap();
}

fn init_repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "master"]);
    write_fixture(dir.path());
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);
    dir
}

#[test]
fn workspace_scan_finds_crates_and_path_deps() {
    let repo = init_repo();
    let ws = scan_workspace(repo.path()).unwrap();
    let names: Vec<_> = ws.crates.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    let alpha = ws.crates.iter().find(|c| c.name == "alpha").unwrap();
    assert!(alpha.dependencies.iter().any(|d| d == "beta"));
}

#[test]
fn index_then_query_architecture_without_rereading_source() {
    let repo = init_repo();
    let store = tempfile::tempdir().unwrap();
    let stats = index(IndexOptions {
        repo_root: repo.path().to_path_buf(),
        store_path: store.path().to_path_buf(),
        name: Some("demo".into()),
    })
    .unwrap();
    assert!(!stats.skipped);
    assert!(stats.symbol_count > 0);
    assert!(stats.files_indexed >= 2);

    let db = codeintel::indexer::open_store(store.path()).unwrap();
    let q = QueryEngine::open(&db, "demo");
    let summary = q.summary().unwrap();
    assert_eq!(summary.crates, 2);
    assert!(summary.symbols > 0);

    let cycles = q.circular_dependencies().unwrap();
    assert!(
        cycles
            .iter()
            .any(|c| c.nodes.contains(&"alpha".into()) && c.nodes.contains(&"beta".into())),
        "expected alpha↔beta cycle, got {cycles:?}"
    );

    let unused = q.unused_public_apis().unwrap();
    assert!(
        unused.iter().any(|u| u.qualname.ends_with("unused_public")),
        "expected unused_public, got {unused:?}"
    );

    let dups = q.duplicates().unwrap();
    assert!(
        dups.iter().any(|g| g.members.len() >= 2),
        "expected duplicated clone_body, got {dups:?}"
    );

    let callers = q.callers("alpha::used").unwrap();
    assert!(!callers.is_empty(), "used() should have a caller");

    let impls = q.trait_impls("alpha::Marker").unwrap();
    assert!(
        !impls.is_empty(),
        "Widget should implement Marker: {impls:?}"
    );

    db.close().unwrap();
}

#[test]
fn second_index_of_same_commit_is_skipped() {
    let repo = init_repo();
    let store = tempfile::tempdir().unwrap();
    let opts = || IndexOptions {
        repo_root: repo.path().to_path_buf(),
        store_path: store.path().to_path_buf(),
        name: Some("demo".into()),
    };
    let first = index(opts()).unwrap();
    assert!(!first.skipped);
    let second = index(opts()).unwrap();
    assert!(second.skipped);
    assert!(second.duration_ms <= first.duration_ms.max(50) + 200);
}

#[test]
fn incremental_indexes_only_changed_file() {
    let repo = init_repo();
    let store = tempfile::tempdir().unwrap();
    let opts = || IndexOptions {
        repo_root: repo.path().to_path_buf(),
        store_path: store.path().to_path_buf(),
        name: Some("demo".into()),
    };
    index(opts()).unwrap();
    fs::write(
        repo.path().join("alpha/src/extra.rs"),
        "pub fn extra() {}\n",
    )
    .unwrap();
    // extra.rs is not in lib.rs but still a new tracked file
    git(repo.path(), &["add", "alpha/src/extra.rs"]);
    git(repo.path(), &["commit", "-m", "add extra"]);
    let inc = index(opts()).unwrap();
    assert!(!inc.skipped);
    assert_eq!(inc.files_indexed, 1);
}
