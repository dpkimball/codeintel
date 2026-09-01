//! Post-index pass: parse `git log` commit subjects for `#NNN` references,
//! then write `REL_RESOLVES` edges from `issue_id(repo_name, N)` to each file
//! touched in that commit. This runs once per index, not once per file.

use crate::error::{CodeintelError, Result};
use crate::ids::{edge_id, file_id, issue_id, ACTOR, REL_RESOLVES};
use bindb::{BinderStore, Provenance, ProvenanceSource, Transaction};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

/// Run `git log` on `repo_path`, find commits whose subjects reference `#NNN`,
/// write `REL_RESOLVES` edges from the indexed issue to each file the commit touched.
///
/// Only writes edges for issues that are already indexed in `store`. Silently
/// skips issues that are not indexed (unresolved `#NNN` references).
pub fn link_issues_from_commits(
    store: &BinderStore,
    repo_name: &str,
    repo_path: &Path,
) -> Result<usize> {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_path.to_string_lossy(),
            "log",
            "--format=%H %s",
            "--name-only",
        ])
        .output()
        .map_err(|e| CodeintelError::Other(format!("git log failed: {e}")))?;

    if !output.status.success() {
        return Err(CodeintelError::Other(format!(
            "git log error: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let provenance = Provenance {
        source: ProvenanceSource::Import("git-log".into()),
        source_ref: repo_name.to_string(),
        imported_at_ms: SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        ..Default::default()
    };

    // Parse: lines starting with 40 hex chars + space are commit headers;
    // other non-empty lines are file paths belonging to the most recent commit.
    let mut current_issues: Vec<u64> = Vec::new();
    let mut current_files: Vec<String> = Vec::new();
    let mut edge_count = 0usize;

    for line in text.lines() {
        if is_commit_header(line) {
            // Flush the previous commit.
            edge_count += write_edges(
                store,
                repo_name,
                &current_issues,
                &current_files,
                &provenance,
            )?;
            let subject = line.split_once(' ').map(|x| x.1).unwrap_or("");
            current_issues = parse_issue_refs(subject);
            current_files = Vec::new();
        } else if !line.trim().is_empty() && !current_issues.is_empty() {
            current_files.push(line.to_string());
        }
    }
    edge_count += write_edges(
        store,
        repo_name,
        &current_issues,
        &current_files,
        &provenance,
    )?;

    store.flush()?;
    Ok(edge_count)
}

/// True when `line` starts with a 40-hex-character SHA followed by a space.
fn is_commit_header(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 41 || bytes[40] != b' ' {
        return false;
    }
    bytes[..40].iter().all(|b: &u8| b.is_ascii_hexdigit())
}

fn write_edges(
    store: &BinderStore,
    repo_name: &str,
    issues: &[u64],
    files: &[String],
    provenance: &Provenance,
) -> Result<usize> {
    if issues.is_empty() || files.is_empty() {
        return Ok(0);
    }
    let mut count = 0;
    for &n in issues {
        let iid = issue_id(repo_name, n);
        // Only write if the issue object is already indexed.
        match store.get(&iid) {
            Ok(Some(_)) => {}
            _ => continue,
        }
        for path in files {
            let fid = file_id(repo_name, path);
            let eid = edge_id(REL_RESOLVES, &iid, &fid);
            let mut tx = Transaction::new();
            tx.put_edge_with_provenance(
                eid,
                iid.clone(),
                fid,
                REL_RESOLVES,
                None,
                Some(provenance.clone()),
            );
            store.commit_buffered(tx, ACTOR, format!("resolves #{n} → {path}"))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Extract issue numbers referenced as `#NNN` (1–5 digits) from a commit subject.
fn parse_issue_refs(subject: &str) -> Vec<u64> {
    let mut nums: BTreeSet<u64> = BTreeSet::new();
    let bytes = subject.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start && end - start <= 5 {
                if let Ok(s) = std::str::from_utf8(&bytes[start..end]) {
                    if let Ok(n) = s.parse::<u64>() {
                        if n > 0 {
                            nums.insert(n);
                        }
                    }
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    nums.into_iter().collect()
}

/// For a set of file paths in the inspection set, return all issue numbers
/// connected via REL_RESOLVES edges pointing to those files.
pub fn origin_issues_for_files(
    store: &BinderStore,
    repo_name: &str,
    file_paths: &[&str],
) -> Result<Vec<u64>> {
    let mut nums: BTreeSet<u64> = BTreeSet::new();
    for path in file_paths {
        let fid = file_id(repo_name, path);
        let edges = match store.edges_to(&fid) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for edge in edges {
            if edge.relationship_type.as_deref() != Some(REL_RESOLVES) {
                continue;
            }
            let Some(from) = &edge.from_object else {
                continue;
            };
            // from = `code://issue/{repo_name}/{number}`
            let prefix = format!("code://issue/{repo_name}/");
            if let Some(n_str) = from.0.strip_prefix(&prefix) {
                if let Ok(n) = n_str.parse::<u64>() {
                    nums.insert(n);
                }
            }
        }
    }
    Ok(nums.into_iter().collect())
}
