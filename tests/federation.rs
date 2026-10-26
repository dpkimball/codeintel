use codeintel::indexer::{index, open_store, IndexOptions};
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

fn init_crate(dir: &Path, name: &str, dep: Option<&str>) {
    fs::create_dir_all(dir.join("src")).unwrap();
    let dep_toml = match dep {
        Some(d) => format!("[dependencies]\n{d} = \"0.1\"\n"),
        None => String::new(),
    };
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{dep_toml}"
        ),
    )
    .unwrap();
    fs::write(dir.join("src/lib.rs"), format!("pub fn {name}_fn() {{}}\n")).unwrap();
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
fn federation_links_named_dep_across_repos_without_source() {
    let lib = tempfile::tempdir().unwrap();
    let app = tempfile::tempdir().unwrap();
    init_crate(lib.path(), "libx", Some("appx"));
    init_crate(app.path(), "appx", Some("libx"));

    let store_dir = tempfile::tempdir().unwrap();
    index(opts(lib.path(), store_dir.path(), "libs")).unwrap();
    index(opts(app.path(), store_dir.path(), "apps")).unwrap();

    let lib_path: PathBuf = lib.keep();
    let app_path: PathBuf = app.keep();
    fs::remove_dir_all(&lib_path).unwrap();
    fs::remove_dir_all(&app_path).unwrap();

    let db = open_store(store_dir.path()).unwrap();
    let report = codeintel::federation_analyze(&db).unwrap();
    assert_eq!(report.source_files_opened, 0);
    assert_eq!(report.primary_source, "bindb");
    assert!(report.repos.iter().any(|r| r == "libs"));
    assert!(report.repos.iter().any(|r| r == "apps"));
    assert!(
        report.cross_repo_deps.iter().any(|d| {
            d.from_repo == "apps"
                && d.from_crate == "appx"
                && d.to_repo == "libs"
                && d.to_crate == "libx"
        }),
        "missing apps/appx → libs/libx: {:?}",
        report.cross_repo_deps
    );
    assert!(
        report.cross_repo_deps.iter().any(|d| {
            d.from_repo == "libs"
                && d.from_crate == "libx"
                && d.to_repo == "apps"
                && d.to_crate == "appx"
        }),
        "missing libs/libx → apps/appx: {:?}",
        report.cross_repo_deps
    );
    assert!(
        !report.cycles.is_empty(),
        "expected a cycle spanning repos: {:?}",
        report.cycles
    );

    let impact = codeintel::federation_impact(&db, "libx").unwrap();
    let by_path = codeintel::federation_impact_paths(&db, "libs", &["src/lib.rs".into()]).unwrap();
    db.close().unwrap();
    assert!(
        by_path
            .files
            .iter()
            .any(|f| f.path == "src/lib.rs" && f.crate_name == "libx"),
        "src/lib.rs should map to crate libx: {:?}",
        by_path.files
    );
    assert!(
        by_path
            .dependents
            .iter()
            .any(|c| c.repo == "apps" && c.name == "appx"),
        "path impact should list apps/appx: {:?}",
        by_path.dependents
    );
    assert!(by_path.unmapped_paths.is_empty());
    assert_eq!(impact.source_files_opened, 0);
    assert!(
        impact
            .dependents
            .iter()
            .any(|c| c.repo == "apps" && c.name == "appx"),
        "libx should list apps/appx as a dependent: {:?}",
        impact.dependents
    );
}

#[test]
fn federation_path_impact_reports_unmapped_without_writing() {
    let lib = tempfile::tempdir().unwrap();
    let app = tempfile::tempdir().unwrap();
    init_crate(lib.path(), "libx", Some("appx"));
    init_crate(app.path(), "appx", Some("libx"));

    let store_dir = tempfile::tempdir().unwrap();
    index(opts(lib.path(), store_dir.path(), "libs")).unwrap();
    index(opts(app.path(), store_dir.path(), "apps")).unwrap();

    let db = open_store(store_dir.path()).unwrap();
    let missing = codeintel::federation_impact_paths(&db, "libs", &["missing.rs".into()]).unwrap();
    db.close().unwrap();
    assert!(missing.files.is_empty());
    assert_eq!(missing.unmapped_paths, vec!["missing.rs".to_string()]);
    assert!(missing.dependents.is_empty());
    assert_eq!(missing.source_files_opened, 0);
}
