//! Index a pre-authored Q&A question bank into bindb for plan() guidance retrieval.
//!
//! Reads `codev/question-bank.md` (3-column markdown tables: | ID | Question | Answer |).
//! Answer cells must not contain `|`. Each row becomes a `QuestionRecord` stored under
//! `code://question/{repo}/{id}` (repo-specific) or `code://question/cross/{id}` (cross-repo).

use crate::architecture::request_tokens;
use crate::error::{CodeintelError, Result};
use crate::ids::{question_id, ACTOR, NAMESPACE};
use crate::indexer::put_json;
use crate::model::QuestionRecord;
use bindb::{BinderStore, Provenance, ProvenanceSource, Transaction};
use std::time::SystemTime;

/// Parse `question-bank.md` and upsert all Q&A rows into the store.
/// Returns the count of questions indexed.
/// Upsert one Q&A row into bindb (same object id as bulk `index-questions`).
pub fn put_question(
    store: &BinderStore,
    repo: &str,
    id: &str,
    question: &str,
    answer: &str,
) -> Result<()> {
    store.register_namespace(NAMESPACE, ACTOR)?;
    let tags = compute_tags(question, answer);
    let rec = QuestionRecord {
        id: id.to_string(),
        repo: repo.to_string(),
        question: question.to_string(),
        answer: answer.to_string(),
        tags,
    };
    let provenance = Provenance {
        source: ProvenanceSource::Import("put-question".into()),
        source_ref: format!("{repo}/{id}"),
        imported_at_ms: SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };
    let oid = question_id(repo, id);
    let mut tx = Transaction::new();
    put_json(&mut tx, oid, &rec, Some(provenance))?;
    store.commit_buffered(tx, ACTOR, format!("question {repo}/{id}"))?;
    Ok(())
}

pub fn index_questions(store: &BinderStore, bank_path: &std::path::Path) -> Result<usize> {
    store.register_namespace(NAMESPACE, ACTOR)?;

    let content = std::fs::read_to_string(bank_path).map_err(|e| {
        CodeintelError::Other(format!(
            "cannot read question bank {}: {e}",
            bank_path.display()
        ))
    })?;

    let records = parse_question_bank(&content);
    if records.is_empty() {
        return Ok(0);
    }

    let provenance = Provenance {
        source: ProvenanceSource::Import("question-bank".into()),
        source_ref: bank_path.display().to_string(),
        imported_at_ms: SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };

    for rec in &records {
        let id = question_id(&rec.repo, &rec.id);
        let mut tx = Transaction::new();
        put_json(&mut tx, id, rec, Some(provenance.clone()))?;
        store.commit_buffered(tx, ACTOR, format!("question {}/{}", rec.repo, rec.id))?;
    }

    Ok(records.len())
}

/// Parse the markdown table rows from the question bank content.
/// Skips header rows, separator rows, section headers, and malformed rows.
/// Repo is inferred from section headers (## codeintel → "codeintel", ## Cross-repo → "").
fn parse_question_bank(content: &str) -> Vec<QuestionRecord> {
    let mut records = Vec::new();
    let mut current_repo = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Section header: ## codeintel, ## binder-ai, ## Cross-repo, etc.
        if trimmed.starts_with("## ") {
            let section = trimmed.trim_start_matches("## ").trim().to_lowercase();
            current_repo = if section.contains("cross") {
                String::new()
            } else {
                // Normalize: "binder-ai", "codeintel", "bindb", etc.
                section.replace(' ', "-")
            };
            continue;
        }

        // Must be a pipe-delimited row
        if !trimmed.starts_with('|') {
            continue;
        }

        let cells: Vec<&str> = trimmed.split('|').map(|c| c.trim()).collect();

        // Need at least: ["", ID, Question, Answer, ""]  → 5 elements
        if cells.len() < 5 {
            continue;
        }

        let id = cells[1];
        let question = cells[2];
        let answer = cells[3];

        // Skip header rows (ID = "#" or "ID") and separator rows ("---")
        if id.is_empty()
            || id == "#"
            || id == "ID"
            || id.starts_with('-')
            || question.starts_with('-')
        {
            continue;
        }
        // Skip rows with empty question or answer
        if question.is_empty() || answer.is_empty() {
            continue;
        }

        let tags = compute_tags(question, answer);

        records.push(QuestionRecord {
            id: id.to_string(),
            repo: current_repo.clone(),
            question: question.to_string(),
            answer: answer.to_string(),
            tags,
        });
    }

    records
}

fn compute_tags(question: &str, answer: &str) -> Vec<String> {
    let combined = format!("{question} {answer}").to_lowercase();
    let mut tags: Vec<String> = request_tokens(&combined)
        .into_iter()
        .filter(|t| t.len() >= 4)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_specific_rows() {
        let md = "## codeintel\n\
                  | ID | Question | Answer |\n\
                  |---|---|---|\n\
                  | C1 | What is the boundary? | codeintel writes to bindb. Do not add WAL here. |\n";
        let recs = parse_question_bank(md);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "C1");
        assert_eq!(recs[0].repo, "codeintel");
        assert!(recs[0].tags.contains(&"boundary".to_string()));
    }

    #[test]
    fn parses_cross_repo_rows() {
        let md = "## Cross-repo\n\
                  | ID | Question | Answer |\n\
                  |---|---|---|\n\
                  | X1 | What is SPIDER? | Specify Plan Implement Deploy Evaluate Retro. |\n";
        let recs = parse_question_bank(md);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "X1");
        assert_eq!(recs[0].repo, "");
    }

    #[test]
    fn put_question_round_trips_in_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        put_question(
            &store,
            "codeintel",
            "T99",
            "What is the write boundary?",
            "codeintel writes to bindb only.",
        )
        .unwrap();
        let q = crate::query::QueryEngine::open(&store, "codeintel");
        let all = q.questions().unwrap();
        assert!(all.iter().any(|r| r.id == "T99" && r.answer.contains("bindb")));
    }

    #[test]
    fn skips_header_and_separator_rows() {
        let md = "## bindb\n\
                  | # | Question | Answer |\n\
                  |---|---|---|\n\
                  | B1 | Real question? | Real answer. |\n";
        let recs = parse_question_bank(md);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "B1");
    }
}
