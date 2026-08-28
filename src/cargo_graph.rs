use crate::error::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CrateInfo {
    pub name: String,
    pub manifest_path: PathBuf,
    pub src_dir: PathBuf,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Default)]
pub struct WorkspaceGraph {
    pub crates: Vec<CrateInfo>,
}

impl WorkspaceGraph {
    pub fn crate_for_file(&self, file: &Path) -> Option<&CrateInfo> {
        let mut best: Option<&CrateInfo> = None;
        let mut best_len = 0usize;
        for c in &self.crates {
            let root = c.manifest_path.parent()?;
            if file.starts_with(root) && root.as_os_str().len() >= best_len {
                best = Some(c);
                best_len = root.as_os_str().len();
            }
        }
        best
    }
}

pub fn scan_workspace(repo_root: &Path) -> Result<WorkspaceGraph> {
    let mut crates = Vec::new();
    visit_manifest(repo_root, &mut crates)?;
    Ok(WorkspaceGraph { crates })
}

fn visit_manifest(dir: &Path, crates: &mut Vec<CrateInfo>) -> Result<()> {
    let manifest = dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&manifest)?;
    let parsed: Manifest = toml::from_str(&text).unwrap_or_default();
    if let Some(pkg) = parsed.package {
        let deps = parsed.dependencies.keys().cloned().collect();
        crates.push(CrateInfo {
            name: pkg.name,
            manifest_path: manifest.clone(),
            src_dir: dir.join("src"),
            dependencies: deps,
        });
    }
    if let Some(ws) = parsed.workspace {
        for member in ws.members {
            if member.contains('*') {
                glob_members(dir, &member, crates)?;
            } else {
                visit_manifest(&dir.join(member), crates)?;
            }
        }
    }
    Ok(())
}

fn glob_members(dir: &Path, pattern: &str, crates: &mut Vec<CrateInfo>) -> Result<()> {
    // Limited glob: prefix/*  or prefix/*/suffix
    let parts: Vec<&str> = pattern.split('/').collect();
    walk_glob(dir, &parts, 0, crates)
}

fn walk_glob(
    current: &Path,
    parts: &[&str],
    idx: usize,
    crates: &mut Vec<CrateInfo>,
) -> Result<()> {
    if idx == parts.len() {
        visit_manifest(current, crates)?;
        return Ok(());
    }
    let part = parts[idx];
    if part == "*" {
        if let Ok(rd) = std::fs::read_dir(current) {
            for ent in rd.flatten() {
                if ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    walk_glob(&ent.path(), parts, idx + 1, crates)?;
                }
            }
        }
    } else {
        walk_glob(&current.join(part), parts, idx + 1, crates)?;
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    package: Option<Package>,
    workspace: Option<Workspace>,
    #[serde(default)]
    dependencies: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct Workspace {
    #[serde(default)]
    members: Vec<String>,
}

/// Rust module path for a source file inside a crate.
pub fn rust_module_path(crate_name: &str, src_dir: &Path, file: &Path) -> String {
    let Ok(rel) = file.strip_prefix(src_dir) else {
        return crate_name.to_string();
    };
    let mut segs: Vec<String> = Vec::new();
    for comp in rel.iter() {
        let s = comp.to_string_lossy();
        if s.ends_with(".rs") {
            let stem = s.trim_end_matches(".rs");
            if stem != "mod" && stem != "lib" && stem != "main" {
                segs.push(stem.to_string());
            }
        } else {
            segs.push(s.to_string());
        }
    }
    if segs.is_empty() {
        crate_name.to_string()
    } else {
        format!("{crate_name}::{}", segs.join("::"))
    }
}
