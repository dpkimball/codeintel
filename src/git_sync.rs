use crate::error::{CodeintelError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitHead {
    pub sha: String,
    pub branch: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedFile {
    pub path: String,
    pub blob_hash: String,
}

#[derive(Debug, Clone)]
pub enum FileDelta {
    Added(TrackedFile),
    Modified(TrackedFile),
    Deleted { path: String },
}

pub fn require_git_repo(root: &Path) -> Result<PathBuf> {
    let out = git(root, &["rev-parse", "--show-toplevel"])?;
    let top = PathBuf::from(out.trim());
    if !top.exists() {
        return Err(CodeintelError::NotGitRepo(root.to_path_buf()));
    }
    Ok(top)
}

pub fn head(root: &Path) -> Result<GitHead> {
    let sha = git(root, &["rev-parse", "HEAD"])?;
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let timestamp = git(root, &["log", "-1", "--format=%cI", "HEAD"])?;
    Ok(GitHead {
        sha: sha.trim().to_string(),
        branch: branch.trim().to_string(),
        timestamp: timestamp.trim().to_string(),
    })
}

pub fn ls_tree(root: &Path, sha: &str) -> Result<Vec<TrackedFile>> {
    let raw = git(root, &["ls-tree", "-r", "--full-tree", sha])?;
    let mut files = Vec::new();
    for line in raw.lines() {
        // 100644 blob <hash>\t<path>
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let mut parts = meta.split_whitespace();
        let _mode = parts.next();
        let kind = parts.next();
        let hash = parts.next();
        if kind != Some("blob") {
            continue;
        }
        let Some(hash) = hash else {
            continue;
        };
        files.push(TrackedFile {
            path: path.to_string(),
            blob_hash: hash.to_string(),
        });
    }
    Ok(files)
}

pub fn diff_trees(old: &[TrackedFile], new: &[TrackedFile]) -> Vec<FileDelta> {
    let old_map: BTreeMap<&str, &str> = old
        .iter()
        .map(|f| (f.path.as_str(), f.blob_hash.as_str()))
        .collect();
    let new_map: BTreeMap<&str, &TrackedFile> = new.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut out = Vec::new();
    for (path, file) in &new_map {
        match old_map.get(path) {
            None => out.push(FileDelta::Added((*file).clone())),
            Some(h) if *h != file.blob_hash.as_str() => {
                out.push(FileDelta::Modified((*file).clone()));
            }
            Some(_) => {}
        }
    }
    for path in old_map.keys() {
        if !new_map.contains_key(path) {
            out.push(FileDelta::Deleted {
                path: (*path).to_string(),
            });
        }
    }
    out
}

pub fn show_file(root: &Path, sha: &str, path: &str) -> Result<Vec<u8>> {
    let spec = format!("{sha}:{path}");
    git_bytes(root, &["show", &spec])
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let bytes = git_bytes(root, args)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(CodeintelError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            err.trim()
        )));
    }
    Ok(output.stdout)
}
