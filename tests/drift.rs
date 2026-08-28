use codeintel::indexer::{index, open_store, IndexOptions};
use std::fs;
use std::path::Path;
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

fn sha(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn write_v1(root: &Path) {
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
    fs::write(root.join("alpha/src/lib.rs"), "pub fn a() {}\n").unwrap();
    fs::create_dir_all(root.join("beta/src")).unwrap();
    fs::write(
        root.join("beta/Cargo.toml"),
        r#"
[package]
name = "beta"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    fs::write(root.join("beta/src/lib.rs"), "pub fn b() {}\n").unwrap();
}

fn write_v2(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        r#"
[workspace]
members = ["alpha", "beta", "gamma"]
"#,
    )
    .unwrap();
    fs::write(
        root.join("alpha/Cargo.toml"),
        r#"
[package]
name = "alpha"
version = "0.1.0"
edition = "2021"
[dependencies]
beta = { path = "../beta" }
gamma = { path = "../gamma" }
"#,
    )
    .unwrap();
    fs::create_dir_all(root.join("gamma/src")).unwrap();
    fs::write(
        root.join("gamma/Cargo.toml"),
        r#"
[package]
name = "gamma"
version = "0.1.0"
edition = "2021"
[dependencies]
alpha = { path = "../alpha" }
"#,
    )
    .unwrap();
    fs::write(root.join("gamma/src/lib.rs"), "pub fn g() {}\n").unwrap();
}

#[test]
fn drift_from_bindb_after_git_fixture_removed() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-b", "master"]);
    write_v1(repo.path());
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "v1"]);
    let sha1 = sha(repo.path());

    let store_dir = tempfile::tempdir().unwrap();
    let opts = |root: &Path| IndexOptions {
        repo_root: root.to_path_buf(),
        store_path: store_dir.path().to_path_buf(),
        name: Some("demo".into()),
    };
    index(opts(repo.path())).unwrap();

    write_v2(repo.path());
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-m", "v2"]);
    let sha2 = sha(repo.path());
    index(opts(repo.path())).unwrap();

    let repo_path = repo.keep();
    fs::remove_dir_all(&repo_path).unwrap();

    let db = open_store(store_dir.path()).unwrap();
    let report = codeintel::drift_analyze(&db, "demo", &sha1, &sha2).unwrap();
    db.close().unwrap();

    assert_eq!(report.source_files_opened, 0);
    assert_eq!(report.primary_source, "bindb");
    assert_eq!(report.from_sha, sha1);
    assert_eq!(report.to_sha, sha2);
    assert!(report.crates_added.contains(&"gamma".to_string()));
    assert!(report.crates_removed.is_empty());
    assert!(report
        .deps_added
        .iter()
        .any(|(a, b)| a == "alpha" && b == "gamma"));
    assert!(report
        .deps_added
        .iter()
        .any(|(a, b)| a == "gamma" && b == "alpha"));
    assert!(
        report
            .doctrine_introduced
            .iter()
            .any(|f| f.rule_id.contains("acyclic")),
        "cycle alpha↔gamma should appear as introduced doctrine: {:?}",
        report.doctrine_introduced
    );
}
