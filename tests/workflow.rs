use codeintel::indexer::{index, IndexOptions};
use codeintel::workflow::{finish, parse_bind, start, ValidationRecord};
use codeintel::Architecture;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn init_two_file_crate(dir: &Path, name: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    fs::write(dir.join("src/lib.rs"), format!("pub fn {name}_fn() {{}}\n")).unwrap();
    fs::write(dir.join("src/extra.rs"), "pub fn leftover() {}\n").unwrap();
    git(dir, &["init", "-b", "master"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-m", "init"]);
}

fn opts(root: &Path, store: &Path, name: &str) -> IndexOptions {
    IndexOptions {
        repo_root: root.to_path_buf(),
        store_path: store.to_path_buf(),
        name: Some(name.into()),
    }
}

#[test]
fn workflow_plans_from_graph_after_trees_removed() {
    let lib = tempfile::tempdir().unwrap();
    init_two_file_crate(lib.path(), "libx");
    let store_dir = tempfile::tempdir().unwrap();
    index(opts(lib.path(), store_dir.path(), "libs")).unwrap();

    let lib_path: PathBuf = lib.keep();
    fs::remove_dir_all(&lib_path).unwrap();

    let session = tempfile::NamedTempFile::new().unwrap();
    let started = start(
        store_dir.path(),
        "change src/lib.rs",
        vec![],
        session.path(),
    )
    .unwrap();
    assert_eq!(started.source_files_opened_during_planning, 0);
    assert_eq!(started.matched_kind, "file");
    assert!(
        started
            .files_identified
            .iter()
            .any(|f| f == "libs:src/lib.rs"),
        "graph should name libs:src/lib.rs: {:?}",
        started.files_identified
    );
    assert!(
        !started
            .files_identified
            .iter()
            .any(|f| f.ends_with("src/extra.rs")),
        "extra.rs is not implicated: {:?}",
        started.files_identified
    );
    assert!(started.graph_file_count > started.files_identified.len());
    assert_eq!(started.doctrine_before[0].findings, 0);

    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch.plan("change src/lib.rs").unwrap();
    assert_eq!(plan.source_files_opened, 0);
    arch.close().unwrap();
}

#[test]
fn workflow_loop_implements_only_inspection_set() {
    let lib = tempfile::tempdir().unwrap();
    init_two_file_crate(lib.path(), "libx");
    let store_dir = tempfile::tempdir().unwrap();
    index(opts(lib.path(), store_dir.path(), "libs")).unwrap();

    let session = tempfile::NamedTempFile::new().unwrap();
    let bind = parse_bind(&format!("libs={}", lib.path().display())).unwrap();
    let started = start(
        store_dir.path(),
        "change src/lib.rs",
        vec![bind],
        session.path(),
    )
    .unwrap();
    assert_eq!(started.matched_kind, "file");
    let identified = started.files_identified.clone();
    assert!(identified.iter().any(|f| f == "libs:src/lib.rs"));
    assert!(!identified.iter().any(|f| f.ends_with("src/extra.rs")));
    assert!(!identified.iter().any(|f| f == "src/extra.rs"));

    fs::write(
        lib.path().join("src/lib.rs"),
        "pub fn libx_fn() {}\npub fn added_from_graph_plan() {}\n",
    )
    .unwrap();
    fs::write(lib.path().join("src/helper.rs"), "pub fn helper() {}\n").unwrap();
    git(lib.path(), &["add", "-A"]);
    git(
        lib.path(),
        &["commit", "-m", "implement from inspection set"],
    );

    let report = finish(
        session.path(),
        vec!["src/lib.rs".into(), "src/helper.rs".into()],
        vec!["src/lib.rs".into(), "src/helper.rs".into()],
        ValidationRecord {
            passed: true,
            detail: "fixture compile not required; graph loop only".into(),
        },
    )
    .unwrap();

    assert_eq!(report.source_files_opened_during_graph_phases, 0);
    assert_eq!(report.primary_source, "bindb");
    assert!(report.planning_ms < 60_000);
    assert!(report.reindex_ms < 60_000);
    assert_eq!(report.repositories_consulted, 1);
    assert!(report.files_identified_by_graph < report.graph_file_count);
    assert_eq!(report.files_opened, 2);
    assert_eq!(report.files_modified, 2);
    assert!(report.files_new.iter().any(|f| f == "src/helper.rs"));
    assert!(report.opened_outside_inspection_set.is_empty());
    assert_eq!(report.doctrine_status_before, "satisfied");
    assert_eq!(report.doctrine_status_after, "satisfied");
    assert!(report.validation.passed);
    assert!(report
        .reindex
        .iter()
        .any(|s| s.name == "libs" && !s.skipped));
    assert!(report.impact_after.is_some());
}

#[test]
fn workflow_flags_opens_outside_inspection_set() {
    let lib = tempfile::tempdir().unwrap();
    init_two_file_crate(lib.path(), "libx");
    let store_dir = tempfile::tempdir().unwrap();
    index(opts(lib.path(), store_dir.path(), "libs")).unwrap();

    let session = tempfile::NamedTempFile::new().unwrap();
    let bind = parse_bind(&format!("libs={}", lib.path().display())).unwrap();
    start(
        store_dir.path(),
        "change src/lib.rs",
        vec![bind],
        session.path(),
    )
    .unwrap();

    let report = finish(
        session.path(),
        vec!["src/lib.rs".into(), "src/extra.rs".into()],
        vec!["src/extra.rs".into()],
        ValidationRecord {
            passed: true,
            detail: "outside open is recorded, not rejected".into(),
        },
    )
    .unwrap();
    assert!(
        report
            .opened_outside_inspection_set
            .iter()
            .any(|f| f == "src/extra.rs"),
        "extra.rs is in the graph but not the inspection set: {:?}",
        report.opened_outside_inspection_set
    );
}

#[test]
fn parse_bind_requires_name_and_path() {
    assert!(parse_bind("libs=/tmp/x").is_ok());
    assert!(parse_bind("nope").is_err());
}
