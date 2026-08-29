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

fn opts(root: &Path, store: &Path, name: &str) -> IndexOptions {
    IndexOptions {
        repo_root: root.to_path_buf(),
        store_path: store.to_path_buf(),
        name: Some(name.into()),
    }
}

fn init_service(dir: &Path, name: &str, extra_args: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join(".github/ISSUE_TEMPLATE")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    fs::write(dir.join("src/lib.rs"), format!("pub fn {name}_fn() {{}}\n")).unwrap();
    fs::write(
        dir.join("dpk.toml"),
        format!(
            "[project]\nname = \"{name}\"\n[docker]\nimage_name = \"{name}\"\nextra_args = \"{extra_args}\"\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("Dockerfile"), "FROM rust:1\n").unwrap();
    fs::write(
        dir.join(".github/ISSUE_TEMPLATE/config.yml"),
        "blank_issues_enabled: false\n",
    )
    .unwrap();
    git(dir, &["init", "-b", "master"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-m", "init"]);
}

fn index_two_and_drop_trees() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let server = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    init_service(
        server.path(),
        "binder-server",
        "--build-arg CARGO_REGISTRIES_DPK_INDEX=sparse+http://host.docker.internal:31127/api/v1/crates/",
    );
    init_service(
        cloud.path(),
        "binder-cloud",
        "--build-context sibling=../binder-ai",
    );
    let store_dir = tempfile::tempdir().unwrap();
    index(opts(server.path(), store_dir.path(), "binder-server")).unwrap();
    index(opts(cloud.path(), store_dir.path(), "binder-cloud")).unwrap();
    let server_path: PathBuf = server.keep();
    let cloud_path: PathBuf = cloud.keep();
    fs::remove_dir_all(&server_path).unwrap();
    fs::remove_dir_all(&cloud_path).unwrap();
    (store_dir, server_path, cloud_path)
}

#[test]
fn colliding_dockerfiles_are_repo_qualified() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch.plan("change Dockerfile").unwrap();
    assert_eq!(plan.source_files_opened, 0);
    assert_eq!(plan.matched_kind, "file");
    assert!(plan
        .smallest_file_set
        .iter()
        .any(|p| p == "binder-server:Dockerfile"));
    assert!(plan
        .smallest_file_set
        .iter()
        .any(|p| p == "binder-cloud:Dockerfile"));
    assert_eq!(
        plan.smallest_file_set
            .iter()
            .filter(|p| p.ends_with("Dockerfile"))
            .count(),
        2
    );
    arch.close().unwrap();
}

#[test]
fn colliding_dpk_toml_does_not_collapse() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch.plan("change dpk.toml").unwrap();
    assert_eq!(plan.matched_kind, "file");
    assert!(plan
        .inspect_before_change
        .iter()
        .any(|p| p == "binder-server:dpk.toml"));
    assert!(plan
        .inspect_before_change
        .iter()
        .any(|p| p == "binder-cloud:dpk.toml"));
    arch.close().unwrap();
}

#[test]
fn extra_args_config_points_at_dpk_toml() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch
        .plan("change extra_args CARGO_REGISTRIES_DPK_INDEX")
        .unwrap();
    assert_eq!(plan.source_files_opened, 0);
    assert_eq!(plan.matched_kind, "config");
    assert!(
        plan.smallest_file_set
            .iter()
            .any(|p| p == "binder-server:dpk.toml"),
        "server extra_args should be implicated: {:?}",
        plan.smallest_file_set
    );
    assert!(
        !plan.smallest_file_set.iter().any(|p| p.contains(".github")),
        "config match must not dump issue templates: {:?}",
        plan.smallest_file_set
    );
    arch.close().unwrap();
}

#[test]
fn crate_named_in_shipping_prose_uses_docker_config_not_crate_dump() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch
        .plan("How should a Python service ship a Rust binary from binder-server")
        .unwrap();
    assert_eq!(plan.matched_kind, "config");
    assert!(
        plan.smallest_file_set
            .iter()
            .any(|p| p == "binder-server:dpk.toml"),
        "{:?}",
        plan.smallest_file_set
    );
    assert!(
        !plan.smallest_file_set.iter().any(|p| p.contains(".github")),
        "inspect must not include .github: {:?}",
        plan.smallest_file_set
    );
    assert!(
        !plan
            .smallest_file_set
            .iter()
            .any(|p| p.ends_with("src/lib.rs")),
        "shipping intent must not dump crate sources: {:?}",
        plan.smallest_file_set
    );
    arch.close().unwrap();
}

#[test]
fn impact_accepts_repo_qualified_path() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let impact = arch.impact("binder-server:src/lib.rs").unwrap();
    assert_eq!(impact.source_files_opened, 0);
    assert!(impact.repositories.contains(&"binder-server".to_string()));
    arch.close().unwrap();
}

#[test]
fn docker_shipping_english_points_at_extra_args() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch
        .plan("How does Binder ship a Rust binary in Docker")
        .unwrap();
    assert_eq!(plan.source_files_opened, 0);
    assert_eq!(plan.matched_kind, "config");
    assert!(
        plan.smallest_file_set
            .iter()
            .any(|p| p == "binder-server:dpk.toml"),
        "{:?}",
        plan.smallest_file_set
    );
    assert!(plan
        .smallest_file_set
        .iter()
        .any(|p| p == "binder-server:Dockerfile"));
    arch.close().unwrap();
}

#[test]
fn change_crate_is_not_stolen_by_docker_sibling_names() {
    let (store_dir, _, _) = index_two_and_drop_trees();
    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch.plan("change binder-server").unwrap();
    assert_eq!(plan.matched_kind, "crate");
    assert!(plan
        .smallest_file_set
        .iter()
        .any(|p| p == "binder-server:src/lib.rs"));
    arch.close().unwrap();
}

#[test]
fn ollama_host_includes_values_overlay() {
    let app = tempfile::tempdir().unwrap();
    fs::create_dir_all(app.path().join("charts/templates")).unwrap();
    fs::write(
        app.path().join("dpk.toml"),
        "[project]\nname = \"dash\"\n[docker]\nimage_name = \"dash\"\n",
    )
    .unwrap();
    fs::write(
        app.path().join("charts/templates/_helpers.tpl"),
        "{{- with .Values.ollama }}\n- name: OLLAMA_HOST\n  value: {{ .host | quote }}\n{{- end }}\n",
    )
    .unwrap();
    fs::write(
        app.path().join("charts/values.yaml"),
        "ollama:\n  host: \"\"\n",
    )
    .unwrap();
    fs::write(
        app.path().join("charts/values-dev.yaml"),
        "ollama:\n  host: \"http://dev-proxies-ollama.dev.svc.cluster.local:11434\"\n",
    )
    .unwrap();
    git(app.path(), &["init", "-b", "master"]);
    git(app.path(), &["add", "-A"]);
    git(app.path(), &["commit", "-m", "init"]);
    let store_dir = tempfile::tempdir().unwrap();
    index(opts(app.path(), store_dir.path(), "dash")).unwrap();
    let kept: PathBuf = app.keep();
    fs::remove_dir_all(&kept).unwrap();

    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch.plan("change OLLAMA_HOST").unwrap();
    assert_eq!(plan.matched_kind, "config");
    for expected in [
        "dash:charts/templates/_helpers.tpl",
        "dash:charts/values.yaml",
        "dash:charts/values-dev.yaml",
    ] {
        assert!(
            plan.smallest_file_set.iter().any(|p| p == expected),
            "missing {expected} in {:?}",
            plan.smallest_file_set
        );
    }
    arch.close().unwrap();
}

#[test]
fn nested_git_subdirectory_indexes_only_that_app() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("apps/dash")).unwrap();
    fs::create_dir_all(root.path().join("apps/other")).unwrap();
    fs::write(
        root.path().join("apps/dash/dpk.toml"),
        "[project]\nname = \"dash\"\n[docker]\nimage_name = \"dash\"\nextra_args = \"--build-context binder-ai=../binder-ai\"\n",
    )
    .unwrap();
    fs::write(root.path().join("apps/dash/Dockerfile"), "FROM python:3\n").unwrap();
    fs::write(
        root.path().join("apps/other/secret.rs"),
        "pub fn leak() {}\n",
    )
    .unwrap();
    git(root.path(), &["init", "-b", "master"]);
    git(root.path(), &["add", "-A"]);
    git(root.path(), &["commit", "-m", "init"]);

    let store_dir = tempfile::tempdir().unwrap();
    index(opts(
        &root.path().join("apps/dash"),
        store_dir.path(),
        "repo-dashboard",
    ))
    .unwrap();
    let kept: PathBuf = root.keep();
    fs::remove_dir_all(&kept).unwrap();

    let arch = Architecture::open(store_dir.path()).unwrap();
    let plan = arch.plan("change extra_args --build-context").unwrap();
    assert_eq!(plan.matched_kind, "config");
    assert!(plan
        .smallest_file_set
        .iter()
        .any(|p| p == "repo-dashboard:dpk.toml"));
    assert!(
        !plan
            .smallest_file_set
            .iter()
            .any(|p| p.contains("secret.rs") || p.contains("apps/other")),
        "sibling tree must not be indexed: {:?}",
        plan.smallest_file_set
    );
    arch.close().unwrap();
}
