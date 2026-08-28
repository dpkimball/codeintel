use codeintel::doctrine::{Doctrine, LayerDoctrine, Rule, Severity};
use codeintel::indexer::{index, open_store, IndexOptions};
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

fn private_calls() {
    used();
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
    fs::write(root.join("beta/src/lib.rs"), "pub fn other() {}\n").unwrap();
}

fn indexed_store() -> (TempDir, bindb::BinderStore) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-b", "master"]);
    write_fixture(repo.path());
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "init"]);
    let store = tempfile::tempdir().unwrap();
    index(IndexOptions {
        repo_root: repo.path().to_path_buf(),
        store_path: store.path().to_path_buf(),
        name: Some("demo".into()),
    })
    .unwrap();
    drop(repo);
    let db = open_store(store.path()).unwrap();
    (store, db)
}

#[test]
fn doctrine_finds_cycle_without_source_tree() {
    let (_keep, db) = indexed_store();
    let q = QueryEngine::open(&db, "demo");
    let report = Doctrine::workspace_default().evaluate(&q).unwrap();
    assert_eq!(report.primary_source, "bindb");
    assert_eq!(report.source_files_opened, 0);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.rule_id == "workspace.acyclic-crates" && f.evidence.detail.contains("cycle")),
        "expected cycle finding, got {:?}",
        report.findings
    );
    db.close().unwrap();
}

#[test]
fn doctrine_layering_uses_graph_edges_only() {
    let (_keep, db) = indexed_store();
    let q = QueryEngine::open(&db, "demo");
    let mut layers = LayerDoctrine::flat(&["alpha".into(), "beta".into()]);
    layers.crate_rank.insert("alpha".into(), 1);
    layers.crate_rank.insert("beta".into(), 0);
    let doctrine = Doctrine {
        name: "test".into(),
        rules: vec![Rule::Layering {
            id: "test.layering".into(),
            layers,
        }],
    };
    let report = doctrine.evaluate(&q).unwrap();
    assert_eq!(report.source_files_opened, 0);
    assert!(
        report.findings.iter().any(|f| {
            f.rule_id == "test.layering" && f.evidence.from == "alpha" && f.evidence.to == "beta"
        }),
        "alpha (deeper) depending on beta (shallower) should violate, got {:?}",
        report.findings
    );
    db.close().unwrap();
}

#[test]
fn impact_of_symbol_names_caller_and_inspection_file_from_bindb() {
    let (_keep, db) = indexed_store();
    let q = QueryEngine::open(&db, "demo");
    let report = codeintel::impact_analyze(&q, "alpha::used").unwrap();
    assert_eq!(report.primary_source, "bindb");
    assert_eq!(report.source_files_opened, 0);
    assert_eq!(report.target_kind, codeintel::impact::TargetKind::Symbol);
    assert!(
        report
            .callers
            .iter()
            .any(|n| n.label.contains("private_calls")),
        "callers: {:?}",
        report.callers
    );
    assert!(
        report
            .inspection_set
            .files
            .iter()
            .any(|f| f.ends_with("alpha/src/lib.rs")),
        "inspection files: {:?}",
        report.inspection_set.files
    );
    assert!(report.inspection_set.files.len() <= 3);
    db.close().unwrap();
}

#[test]
fn impact_of_crate_reports_transitive_dependents() {
    let (_keep, db) = indexed_store();
    let q = QueryEngine::open(&db, "demo");
    let report = codeintel::impact_analyze(&q, "alpha").unwrap();
    assert_eq!(report.source_files_opened, 0);
    assert!(
        report
            .transitive_crate_dependents
            .iter()
            .any(|c| c == "beta"),
        "beta depends on alpha: {:?}",
        report.transitive_crate_dependents
    );
    db.close().unwrap();
}

#[test]
fn ownership_rule_emits_severity() {
    let (_keep, db) = indexed_store();
    let q = QueryEngine::open(&db, "demo");
    let doctrine = Doctrine {
        name: "own".into(),
        rules: vec![Rule::Ownership {
            id: "own.alpha".into(),
            owners: vec!["alpha".into()],
            may_depend_on: vec![],
            severity: Severity::Warning,
        }],
    };
    let report = doctrine.evaluate(&q).unwrap();
    assert!(
        report.findings.iter().any(|f| {
            f.rule_id == "own.alpha" && f.severity == Severity::Warning && f.evidence.to == "beta"
        }),
        "{:?}",
        report.findings
    );
    db.close().unwrap();
}
