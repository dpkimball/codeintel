use codeintel::indexer::{index, IndexOptions};
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
fn architecture_api_reasons_from_bindb_after_trees_removed() {
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

    let arch = Architecture::open(store_dir.path()).unwrap();
    let summary = arch.summary().unwrap();
    assert_eq!(summary.source_files_opened, 0);
    assert!(summary.repositories.iter().any(|r| r.name == "libs"));
    assert!(summary.repositories.iter().any(|r| r.name == "apps"));

    let doctrine = arch.doctrine().unwrap();
    assert_eq!(doctrine.source_files_opened, 0);

    let impact = arch.impact("libx").unwrap();
    assert_eq!(impact.source_files_opened, 0);
    assert!(impact.repositories.contains(&"libs".to_string()));

    let deps = arch.dependencies("appx").unwrap();
    assert_eq!(deps.source_files_opened, 0);

    let cycles = arch.cycles().unwrap();
    assert!(cycles.iter().any(|c| c.scope == "federation"));

    let fed = arch.federation().unwrap();
    assert_eq!(fed.source_files_opened, 0);
    assert!(fed
        .shared_libraries
        .iter()
        .any(|c| c.name == "libx" || c.name == "appx"));

    let plan = arch
        .plan("smallest file set to change libx while keeping doctrine")
        .unwrap();
    assert_eq!(plan.source_files_opened, 0);
    assert!(plan.interpretation.contains("libx"));
    assert!(!plan.smallest_file_set.is_empty());
    assert!(!plan.rules_must_remain_satisfied.is_empty());
    arch.close().unwrap();
}
