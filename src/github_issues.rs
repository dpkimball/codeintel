//! Fetch and index GitHub issues into bindb for semantic plan() matching.
//!
//! Requires `gh` CLI to be authenticated, or `GITHUB_TOKEN` env var + `curl`.
//! Issues are stored per codeintel repo name under `code://issue/{repo_name}/{number}`
//! and queried during `plan()` to surface linked_issues alongside the inspection set.

use crate::error::{CodeintelError, Result};
use crate::ids::{issue_id, ACTOR, NAMESPACE};
use crate::indexer::put_json;
use crate::model::IssueRecord;
use bindb::{BinderStore, Provenance, ProvenanceSource, Transaction};
use std::collections::BTreeSet;
use std::process::Command;
use std::time::SystemTime;

/// Fetch all issues for `github_repo` (e.g. "dpkimball/binder-ai") and index them
/// into `store` under the codeintel `repo_name`.
///
/// Tries `gh issue list` first; falls back to `curl` + `GITHUB_TOKEN` env var.
pub fn index_issues(store: &BinderStore, github_repo: &str, repo_name: &str) -> Result<usize> {
    store.register_namespace(NAMESPACE, ACTOR)?;

    let issue_bytes = fetch_issues(github_repo)?;

    let json: serde_json::Value = serde_json::from_slice(&issue_bytes)
        .map_err(|e| CodeintelError::Other(format!("issue JSON parse error: {e}")))?;

    let issues = json
        .as_array()
        .ok_or_else(|| CodeintelError::Other("issue response is not an array".into()))?;

    let provenance = Provenance {
        source: ProvenanceSource::Import("github".into()),
        source_ref: github_repo.to_string(),
        imported_at_ms: SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };

    let mut count = 0;
    for issue_val in issues {
        let number = match issue_val["number"].as_u64() {
            Some(n) if n > 0 => n,
            _ => continue,
        };
        // pull_request field is present on PRs — skip them
        if !issue_val["pull_request"].is_null() {
            continue;
        }
        let title = issue_val["title"].as_str().unwrap_or("").to_string();
        let body: String = issue_val["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(2000)
            .collect();
        // gh uses "url"; REST API uses "html_url"
        let url = issue_val["url"]
            .as_str()
            .or_else(|| issue_val["html_url"].as_str())
            .unwrap_or("")
            .to_string();
        let state = issue_val["state"].as_str().unwrap_or("").to_lowercase();
        let labels: Vec<String> = issue_val["labels"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let tokens = extract_issue_tokens(&title, &body);

        let rec = IssueRecord {
            number,
            title,
            body,
            url,
            state,
            labels,
            github_repo: github_repo.to_string(),
            repo_name: repo_name.to_string(),
            tokens,
        };

        let mut tx = Transaction::new();
        put_json(
            &mut tx,
            issue_id(repo_name, number),
            &rec,
            Some(provenance.clone()),
        )?;
        store.commit_buffered(tx, ACTOR, format!("issue {repo_name}#{number}"))?;
        count += 1;
    }

    store.flush()?;
    Ok(count)
}

/// Fetch raw issue JSON — tries `gh` first, then `curl` with GITHUB_TOKEN.
fn fetch_issues(github_repo: &str) -> Result<Vec<u8>> {
    // Try gh CLI (authenticated, handles pagination)
    let gh = Command::new("gh")
        .args([
            "issue",
            "list",
            "--repo",
            github_repo,
            "--json",
            "number,title,body,state,labels,url",
            "--limit",
            "500",
            "--state",
            "all",
        ])
        .output();

    if let Ok(out) = gh {
        if out.status.success() {
            return Ok(out.stdout);
        }
    }

    // Fall back to GitHub REST API via curl
    let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "X-GitHub-Api-Version: 2022-11-28",
    ]);
    if !token.is_empty() {
        cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    let url = format!("https://api.github.com/repos/{github_repo}/issues?state=all&per_page=100");
    cmd.arg(&url);

    let out = cmd
        .output()
        .map_err(|e| CodeintelError::Other(format!("curl not available: {e}")))?;

    if !out.status.success() {
        return Err(CodeintelError::Other(format!(
            "curl {url} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(out.stdout)
}

fn extract_issue_tokens(title: &str, body: &str) -> Vec<String> {
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    for word in
        format!("{title} {body}").split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
    {
        let w = word.to_lowercase();
        let w = w.trim_matches(|c: char| c == '-' || c == '_');
        if w.len() >= 4 || crate::ids::SHORT_TECHNICAL_TERMS.contains(&w) {
            tokens.insert(w.to_string());
        }
    }
    tokens.into_iter().collect()
}
