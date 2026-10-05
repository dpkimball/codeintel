//! Fetch and index GitHub issues and PRs into bindb for semantic plan() matching.
//!
//! Requires `gh` CLI to be authenticated, or `GITHUB_TOKEN` env var + `curl`.
//! Issues are stored under `code://issue/{repo_name}/{number}`.
//! PRs are stored under `code://pr/{repo_name}/{number}`.
//! Both are queried during `plan()` to surface linked results alongside the inspection set.

use crate::error::{CodeintelError, Result};
use crate::ids::{issue_id, pr_id, ACTOR, NAMESPACE};
use crate::indexer::put_json;
use crate::model::{IssueRecord, PullRequestRecord};
use bindb::{BinderStore, Provenance, ProvenanceSource, Transaction};
use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::SystemTime;

/// Fetch all issues for `github_repo` (e.g. "owner/repo") and index them
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
        ..Default::default()
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

    let url = format!("https://api.github.com/repos/{github_repo}/issues?state=all&per_page=100");
    github_curl(&url)
}

/// Fetch all pull requests for `github_repo` and index them into `store`.
///
/// Indexes title + body only (review comments require per-PR API calls; tracked separately).
/// Tries `gh` first; falls back to `curl` + `GITHUB_TOKEN`.
pub fn index_prs(store: &BinderStore, github_repo: &str, repo_name: &str) -> Result<usize> {
    store.register_namespace(NAMESPACE, ACTOR)?;

    let pr_bytes = fetch_prs(github_repo)?;

    let json: serde_json::Value = serde_json::from_slice(&pr_bytes)
        .map_err(|e| CodeintelError::Other(format!("PR JSON parse error: {e}")))?;

    let prs = json
        .as_array()
        .ok_or_else(|| CodeintelError::Other("PR response is not an array".into()))?;

    let provenance = Provenance {
        source: ProvenanceSource::Import("github".into()),
        source_ref: github_repo.to_string(),
        imported_at_ms: SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        ..Default::default()
    };

    let mut count = 0;
    for pr_val in prs {
        let number = match pr_val["number"].as_u64() {
            Some(n) if n > 0 => n,
            _ => continue,
        };
        let title = pr_val["title"].as_str().unwrap_or("").to_string();
        let body: String = pr_val["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(2000)
            .collect();
        let url = pr_val["url"]
            .as_str()
            .or_else(|| pr_val["html_url"].as_str())
            .unwrap_or("")
            .to_string();
        let merged = pr_val["mergedAt"].is_string() || pr_val["merged_at"].is_string();
        let state = if merged {
            "merged".to_string()
        } else {
            pr_val["state"].as_str().unwrap_or("").to_lowercase()
        };
        let labels: Vec<String> = pr_val["labels"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let tokens = extract_issue_tokens(&title, &body);

        let rec = PullRequestRecord {
            number,
            title,
            body,
            url,
            state,
            labels,
            merged,
            github_repo: github_repo.to_string(),
            repo_name: repo_name.to_string(),
            tokens,
        };

        let mut tx = Transaction::new();
        put_json(
            &mut tx,
            pr_id(repo_name, number),
            &rec,
            Some(provenance.clone()),
        )?;
        store.commit_buffered(tx, ACTOR, format!("pr {repo_name}#{number}"))?;
        count += 1;
    }

    store.flush()?;
    Ok(count)
}

/// Fetch raw PR JSON — tries `gh` first, then `curl` with GITHUB_TOKEN.
fn fetch_prs(github_repo: &str) -> Result<Vec<u8>> {
    let gh = Command::new("gh")
        .args([
            "pr",
            "list",
            "--repo",
            github_repo,
            "--json",
            "number,title,body,state,labels,url,mergedAt",
            "--limit",
            "200",
            "--state",
            "all",
        ])
        .output();

    if let Ok(out) = gh {
        if out.status.success() {
            return Ok(out.stdout);
        }
    }

    let url = format!("https://api.github.com/repos/{github_repo}/pulls?state=all&per_page=100");
    github_curl(&url)
}

/// curl argv never includes the token. The bearer header is written to stdin
/// via `curl -K -` so it does not appear in the process list.
const CURL_ARGS: &[&str] = &["-sS", "-K", "-"];

fn github_curl(url: &str) -> Result<Vec<u8>> {
    let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
    if token.bytes().any(|b| b == b'"' || b == b'\n' || b == b'\r') {
        return Err(CodeintelError::Other(
            "GITHUB_TOKEN contains characters that cannot be passed safely".into(),
        ));
    }
    let mut child = Command::new("curl")
        .args(CURL_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CodeintelError::Other(format!("curl not available: {e}")))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| CodeintelError::Other("curl stdin unavailable".into()))?;
        stdin
            .write_all(curl_config(&token, url).as_bytes())
            .map_err(|e| CodeintelError::Other(format!("curl stdin write failed: {e}")))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| CodeintelError::Other(format!("curl failed: {e}")))?;
    if !out.status.success() {
        let stderr = redact_token(&String::from_utf8_lossy(&out.stderr), &token);
        return Err(CodeintelError::Other(format!(
            "curl {url} failed: {stderr}"
        )));
    }
    Ok(out.stdout)
}

fn curl_config(token: &str, url: &str) -> String {
    let mut config = String::from(
        "header = \"Accept: application/vnd.github+json\"\n\
         header = \"X-GitHub-Api-Version: 2022-11-28\"\n",
    );
    if !token.is_empty() {
        config.push_str(&format!("header = \"Authorization: Bearer {token}\"\n"));
    }
    config.push_str(&format!("url = \"{url}\"\n"));
    config
}

fn redact_token(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    text.replace(token, "[redacted]")
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

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_TOKEN: &str = "ghp_fakeTokenValue123456";

    #[test]
    fn curl_args_omit_token() {
        let joined = CURL_ARGS.join(" ");
        assert!(!joined.contains(FAKE_TOKEN));
        assert!(!joined.to_lowercase().contains("authorization"));
        assert!(!joined.to_lowercase().contains("bearer"));
    }

    #[test]
    fn curl_config_keeps_token_off_the_argv_and_errors_do_not_echo_it() {
        let url = "https://api.github.com/repos/example/repo/issues?state=all&per_page=100";
        let config = curl_config(FAKE_TOKEN, url);
        assert!(config.contains(FAKE_TOKEN));
        let argv = CURL_ARGS.join(" ");
        assert!(!argv.contains(FAKE_TOKEN));

        let err = redact_token(
            &format!("curl: (6) bearer {FAKE_TOKEN} could not resolve"),
            FAKE_TOKEN,
        );
        assert!(!err.contains(FAKE_TOKEN));
        assert!(err.contains("[redacted]"));
    }

    #[test]
    fn pr_curl_config_also_keeps_token_out_of_argv() {
        let url = "https://api.github.com/repos/example/repo/pulls?state=all&per_page=100";
        let config = curl_config(FAKE_TOKEN, url);
        assert!(config.contains("pulls?"));
        assert!(config.contains(FAKE_TOKEN));
        assert!(!CURL_ARGS.join(" ").contains(FAKE_TOKEN));
    }
}
