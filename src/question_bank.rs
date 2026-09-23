//! Index a pre-authored Q&A question bank into bindb for plan() guidance retrieval.
//!
//! Reads `codev/question-bank.md` (3-column markdown tables: | ID | Question | Answer |).
//! Answer cells must not contain `|`. Each row becomes a `QuestionRecord` stored under
//! `code://question/{repo}/{id}` (repo-specific) or `code://question/cross/{id}` (cross-repo).

use crate::architecture::request_tokens;
use crate::error::{CodeintelError, Result};
use crate::ids::{question_id, ACTOR, NAMESPACE, PREFIX_ALL_QUESTIONS};
use crate::indexer::{decode_obj, put_json};
use crate::model::{QuestionRecord, QuestionStatus};
use bindb::{BinderStore, Provenance, ProvenanceSource, Transaction};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::SystemTime;

pub const DELETED_TOMBSTONE: &str = "[deleted]";

/// Options for S1b lifecycle writes (`reason`, optimistic `expected_version`).
#[derive(Debug, Clone, Default)]
pub struct QuestionWriteOpts<'a> {
    pub reason: Option<&'a str>,
    pub expected_version: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexQuestionsReport {
    pub indexed: usize,
    pub synced: usize,
    pub conflicts: usize,
    /// Rows parsed from the bank markdown file.
    pub rows_parsed: usize,
    /// Rows successfully indexed or provenance-synced.
    pub rows_active: usize,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Normalized content hash for import conflict detection.
pub fn content_hash(question: &str, answer: &str) -> String {
    let normalized = format!("{}|{}", normalize_field(question), normalize_field(answer));
    let digest = Sha256::digest(normalized.as_bytes());
    format!("{:x}", digest)
}

fn normalize_field(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn import_provenance(bank_path: &str, content_hash: &str, import_base_version: u64) -> Provenance {
    Provenance {
        source: ProvenanceSource::Import("question-bank".into()),
        source_ref: bank_path.to_string(),
        imported_at_ms: now_ms(),
        lifecycle_reason: None,
        source_content_hash: Some(content_hash.to_string()),
        import_base_version: Some(import_base_version),
    }
}

fn lifecycle_provenance(reason: &str, repo: &str, id: &str) -> Provenance {
    Provenance {
        source: ProvenanceSource::Import("lifecycle".into()),
        source_ref: format!("{repo}/{id}"),
        imported_at_ms: now_ms(),
        lifecycle_reason: Some(reason.to_string()),
        source_content_hash: None,
        import_base_version: None,
    }
}

fn head_import_meta(store: &BinderStore, repo: &str, id: &str) -> (Option<String>, Option<u64>) {
    let oid = question_id(repo, id);
    let Ok(Some(head)) = store.get(&oid) else {
        return (None, None);
    };
    match head.provenance {
        Some(p) => (p.source_content_hash, p.import_base_version),
        None => (None, None),
    }
}

fn commit_record(
    store: &BinderStore,
    rec: &QuestionRecord,
    provenance: Provenance,
    commit_msg: &str,
) -> Result<()> {
    store.register_namespace(NAMESPACE, ACTOR)?;
    let oid = question_id(&rec.repo, &rec.id);
    let mut tx = Transaction::new();
    put_json(&mut tx, oid, rec, Some(provenance))?;
    store.commit_buffered(tx, ACTOR, commit_msg)?;
    Ok(())
}

fn check_expected_version(
    existing: Option<&QuestionRecord>,
    expected: Option<u64>,
    repo: &str,
    id: &str,
) -> Result<()> {
    let Some(rec) = existing else {
        return Ok(());
    };
    let Some(expected) = expected else {
        return Err(CodeintelError::Other(format!(
            "expected_version required for existing record {repo}/{id}"
        )));
    };
    if expected != rec.version {
        return Err(CodeintelError::StaleVersion {
            repo: repo.to_string(),
            id: id.to_string(),
            current_version: rec.version,
        });
    }
    Ok(())
}

fn require_reason<'a>(reason: Option<&'a str>, op: &str) -> Result<&'a str> {
    reason.ok_or_else(|| CodeintelError::Other(format!("reason required for {op}")))
}

/// Upsert one Q&A row (create without `expected_version`; update requires it in S1b).
pub fn put_question(
    store: &BinderStore,
    repo: &str,
    id: &str,
    question: &str,
    answer: &str,
) -> Result<QuestionRecord> {
    put_question_with_opts(
        store,
        repo,
        id,
        question,
        answer,
        QuestionWriteOpts {
            reason: None,
            expected_version: None,
        },
    )
}

pub fn put_question_with_opts(
    store: &BinderStore,
    repo: &str,
    id: &str,
    question: &str,
    answer: &str,
    opts: QuestionWriteOpts<'_>,
) -> Result<QuestionRecord> {
    let existing = get_question(store, repo, id)?;
    if let Some(mut rec) = existing {
        let reason = require_reason(opts.reason, "update")?;
        check_expected_version(Some(&rec), opts.expected_version, repo, id)?;
        rec.question = question.to_string();
        rec.answer = answer.to_string();
        rec.tags = compute_tags(question, answer, &rec.aliases);
        rec.version += 1;
        commit_record(
            store,
            &rec,
            lifecycle_provenance(reason, repo, id),
            &format!("question put {repo}/{id}"),
        )?;
        Ok(rec)
    } else {
        let reason = opts.reason.unwrap_or("create");
        let rec = QuestionRecord {
            id: id.to_string(),
            repo: repo.to_string(),
            question: question.to_string(),
            answer: answer.to_string(),
            tags: compute_tags(question, answer, &[]),
            status: QuestionStatus::Active,
            version: 1,
            aliases: Vec::new(),
        };
        commit_record(
            store,
            &rec,
            lifecycle_provenance(reason, repo, id),
            &format!("question create {repo}/{id}"),
        )?;
        Ok(rec)
    }
}

/// Stable id for a plan-learned row. Same normalized question always maps here.
pub fn learned_question_id(question: &str) -> String {
    let normalized = normalize_field(question).to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    let hex = format!("{:x}", digest);
    format!("L-{}", &hex[..8])
}

/// Create or refresh an Active row learned from the indexed doc graph.
///
/// An unchanged question and answer does not bump `version`. Updates send the
/// current version as `expected_version`.
pub fn learn_question(
    store: &BinderStore,
    repo: &str,
    question: &str,
    answer: &str,
) -> Result<QuestionRecord> {
    let id = learned_question_id(question);
    if let Some(existing) = get_question(store, repo, &id)? {
        if existing.status == QuestionStatus::Deleted {
            return Err(CodeintelError::Other(format!(
                "question deleted (irreversible): {repo}/{id}"
            )));
        }
        if existing.status == QuestionStatus::Active
            && existing.question == question
            && existing.answer == answer
        {
            return Ok(existing);
        }
        return put_question_with_opts(
            store,
            repo,
            &id,
            question,
            answer,
            QuestionWriteOpts {
                reason: Some("learned from indexed documents"),
                expected_version: Some(existing.version),
            },
        );
    }
    put_question_with_opts(
        store,
        repo,
        &id,
        question,
        answer,
        QuestionWriteOpts {
            reason: Some("learned from indexed documents"),
            expected_version: None,
        },
    )
}

/// Update only the answer; question text preserved.
pub fn patch_question_answer(
    store: &BinderStore,
    repo: &str,
    id: &str,
    answer: &str,
) -> Result<QuestionRecord> {
    patch_question_with_opts(
        store,
        repo,
        id,
        Some(answer),
        None,
        QuestionWriteOpts {
            reason: None,
            expected_version: None,
        },
    )
}

pub fn patch_question_with_opts(
    store: &BinderStore,
    repo: &str,
    id: &str,
    answer: Option<&str>,
    aliases_append: Option<&[String]>,
    opts: QuestionWriteOpts<'_>,
) -> Result<QuestionRecord> {
    if answer.is_none() && aliases_append.map(|a| a.is_empty()).unwrap_or(true) {
        return Err(CodeintelError::Other(
            "patch requires at least one of answer or aliases".into(),
        ));
    }
    let reason = require_reason(opts.reason, "patch")?;
    let existing = get_question(store, repo, id)?
        .ok_or_else(|| CodeintelError::Other(format!("question not found: {repo}/{id}")))?;
    if existing.status == QuestionStatus::Deleted {
        return Err(CodeintelError::Other(format!(
            "question deleted (irreversible): {repo}/{id}"
        )));
    }
    check_expected_version(Some(&existing), opts.expected_version, repo, id)?;
    let mut rec = existing;
    if let Some(answer) = answer {
        rec.answer = answer.to_string();
    }
    if let Some(append) = aliases_append {
        for alias in append {
            let trimmed = alias.trim();
            if trimmed.is_empty() {
                continue;
            }
            let dup = rec.aliases.iter().any(|a| a.eq_ignore_ascii_case(trimmed));
            if !dup {
                rec.aliases.push(trimmed.to_string());
            }
        }
    }
    rec.tags = compute_tags(&rec.question, &rec.answer, &rec.aliases);
    rec.version += 1;
    commit_record(
        store,
        &rec,
        lifecycle_provenance(reason, repo, id),
        &format!("question patch {repo}/{id}"),
    )?;
    Ok(rec)
}

pub fn retire_question(
    store: &BinderStore,
    repo: &str,
    id: &str,
    opts: QuestionWriteOpts<'_>,
) -> Result<QuestionRecord> {
    let reason = require_reason(opts.reason, "retire")?;
    let existing = get_question(store, repo, id)?
        .ok_or_else(|| CodeintelError::Other(format!("question not found: {repo}/{id}")))?;
    if existing.status == QuestionStatus::Deleted {
        return Err(CodeintelError::Other(format!(
            "cannot retire deleted question: {repo}/{id}"
        )));
    }
    check_expected_version(Some(&existing), opts.expected_version, repo, id)?;
    if existing.status == QuestionStatus::Retired {
        return Ok(existing);
    }
    let mut rec = existing;
    rec.status = QuestionStatus::Retired;
    rec.version += 1;
    commit_record(
        store,
        &rec,
        lifecycle_provenance(reason, repo, id),
        &format!("question retire {repo}/{id}"),
    )?;
    Ok(rec)
}

pub fn reactivate_question(
    store: &BinderStore,
    repo: &str,
    id: &str,
    opts: QuestionWriteOpts<'_>,
) -> Result<QuestionRecord> {
    let reason = require_reason(opts.reason, "reactivate")?;
    let existing = get_question(store, repo, id)?
        .ok_or_else(|| CodeintelError::Other(format!("question not found: {repo}/{id}")))?;
    if existing.status == QuestionStatus::Deleted {
        return Err(CodeintelError::Other(format!(
            "cannot reactivate deleted question: {repo}/{id}"
        )));
    }
    check_expected_version(Some(&existing), opts.expected_version, repo, id)?;
    if existing.status == QuestionStatus::Active {
        return Ok(existing);
    }
    let mut rec = existing;
    rec.status = QuestionStatus::Active;
    rec.version += 1;
    commit_record(
        store,
        &rec,
        lifecycle_provenance(reason, repo, id),
        &format!("question reactivate {repo}/{id}"),
    )?;
    Ok(rec)
}

pub fn delete_question(
    store: &BinderStore,
    repo: &str,
    id: &str,
    opts: QuestionWriteOpts<'_>,
) -> Result<QuestionRecord> {
    let reason = require_reason(opts.reason, "delete")?;
    let existing = get_question(store, repo, id)?
        .ok_or_else(|| CodeintelError::Other(format!("question not found: {repo}/{id}")))?;
    check_expected_version(Some(&existing), opts.expected_version, repo, id)?;
    if existing.status == QuestionStatus::Deleted {
        return Ok(existing);
    }
    let mut rec = existing;
    rec.status = QuestionStatus::Deleted;
    rec.answer = DELETED_TOMBSTONE.to_string();
    rec.tags = compute_tags(&rec.question, DELETED_TOMBSTONE, &[]);
    rec.version += 1;
    commit_record(
        store,
        &rec,
        lifecycle_provenance(reason, repo, id),
        &format!("question delete {repo}/{id}"),
    )?;
    Ok(rec)
}

/// Load one indexed question by repo + id.
pub fn get_question(store: &BinderStore, repo: &str, id: &str) -> Result<Option<QuestionRecord>> {
    let oid = question_id(repo, id);
    match store.get(&oid) {
        Ok(Some(v)) => Ok(Some(decode_obj(&v)?)),
        Ok(None) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn upsert_import_row(
    store: &BinderStore,
    rec: &QuestionRecord,
    bank_path: &str,
    force: bool,
) -> Result<&'static str> {
    let incoming_hash = content_hash(&rec.question, &rec.answer);
    let existing = get_question(store, &rec.repo, &rec.id)?;
    let Some(mut current) = existing else {
        let mut created = rec.clone();
        created.version = 1;
        created.status = QuestionStatus::Active;
        commit_record(
            store,
            &created,
            import_provenance(bank_path, &incoming_hash, 1),
            &format!("question import {}/{}", rec.repo, rec.id),
        )?;
        return Ok("indexed");
    };

    if current.status == QuestionStatus::Deleted {
        if force {
            return Err(CodeintelError::Other(format!(
                "import --force cannot reactivate deleted {}/{}",
                rec.repo, rec.id
            )));
        }
        // Git bank is source of truth: row present in markdown reactivates deleted bindb rows.
        current.status = QuestionStatus::Active;
        current.question = rec.question.clone();
        current.answer = rec.answer.clone();
        current.tags = rec.tags.clone();
        current.aliases = rec.aliases.clone();
        current.version += 1;
        commit_record(
            store,
            &current,
            import_provenance(bank_path, &incoming_hash, current.version),
            &format!("question import reactivate {}/{}", rec.repo, rec.id),
        )?;
        return Ok("indexed");
    }

    if current.status == QuestionStatus::Retired {
        current.status = QuestionStatus::Active;
        current.question = rec.question.clone();
        current.answer = rec.answer.clone();
        current.tags = rec.tags.clone();
        current.aliases = rec.aliases.clone();
        current.version += 1;
        commit_record(
            store,
            &current,
            import_provenance(bank_path, &incoming_hash, current.version),
            &format!("question import reactivate {}/{}", rec.repo, rec.id),
        )?;
        return Ok("indexed");
    }

    let canonical_hash = content_hash(&current.question, &current.answer);
    if incoming_hash == canonical_hash {
        if current.aliases != rec.aliases {
            current.aliases = rec.aliases.clone();
            current.tags = rec.tags.clone();
            current.version += 1;
            commit_record(
                store,
                &current,
                import_provenance(bank_path, &incoming_hash, current.version),
                &format!("question import alias sync {}/{}", rec.repo, rec.id),
            )?;
            return Ok("indexed");
        }
        let (_, _) = head_import_meta(store, &rec.repo, &rec.id);
        commit_record(
            store,
            &current,
            import_provenance(bank_path, &incoming_hash, current.version),
            &format!("question import sync {}/{}", rec.repo, rec.id),
        )?;
        return Ok("synced");
    }

    let (_, import_base) = head_import_meta(store, &rec.repo, &rec.id);
    // Rows created via lifecycle API (put/correct) have no import anchor — git bank wins on refresh.
    let base_ok = import_base.map(|b| b == current.version).unwrap_or(true);
    if base_ok || force {
        current.question = rec.question.clone();
        current.answer = rec.answer.clone();
        current.tags = rec.tags.clone();
        current.aliases = rec.aliases.clone();
        current.version += 1;
        commit_record(
            store,
            &current,
            import_provenance(bank_path, &incoming_hash, current.version),
            &format!("question import update {}/{}", rec.repo, rec.id),
        )?;
        return Ok("indexed");
    }

    eprintln!(
        "codeintel: import conflict: {}/{} bindb version {} import_base {:?}",
        rec.repo, rec.id, current.version, import_base
    );
    Ok("conflict")
}

pub fn index_questions(store: &BinderStore, bank_path: &std::path::Path) -> Result<usize> {
    Ok(index_questions_with_opts(store, bank_path, false)?.indexed)
}

pub fn index_questions_with_opts(
    store: &BinderStore,
    bank_path: &std::path::Path,
    force: bool,
) -> Result<IndexQuestionsReport> {
    store.register_namespace(NAMESPACE, ACTOR)?;

    let content = std::fs::read_to_string(bank_path).map_err(|e| {
        CodeintelError::Other(format!(
            "cannot read question bank {}: {e}",
            bank_path.display()
        ))
    })?;

    let records = parse_question_bank(&content);
    let bank_ref = bank_path.display().to_string();
    let mut report = IndexQuestionsReport {
        rows_parsed: records.len(),
        ..IndexQuestionsReport::default()
    };

    for rec in &records {
        match upsert_import_row(store, rec, &bank_ref, force)? {
            "indexed" => report.indexed += 1,
            "synced" => report.synced += 1,
            "conflict" => report.conflicts += 1,
            _ => {}
        }
    }
    report.rows_active = report.indexed + report.synced;

    Ok(report)
}

/// Write question-bank markdown and refresh import provenance at current version.
pub fn export_questions(store: &BinderStore, bank_path: &std::path::Path) -> Result<usize> {
    store.register_namespace(NAMESPACE, ACTOR)?;
    let mut by_repo: BTreeMap<String, Vec<QuestionRecord>> = BTreeMap::new();
    for v in store.list(PREFIX_ALL_QUESTIONS)? {
        let rec: QuestionRecord = decode_obj(&v)?;
        if rec.status == QuestionStatus::Deleted {
            continue;
        }
        by_repo.entry(rec.repo.clone()).or_default().push(rec);
    }

    let mut lines = Vec::new();
    lines.push("# Question bank".to_string());
    lines.push(String::new());
    let mut count = 0usize;

    for (repo, mut recs) in by_repo {
        let section = if repo.is_empty() {
            "## Cross-repo".to_string()
        } else {
            format!("## {repo}")
        };
        lines.push(section);
        let section_has_aliases = recs.iter().any(|r| !r.aliases.is_empty());
        if section_has_aliases {
            lines.push("| ID | Question | Answer | Aliases |".to_string());
            lines.push("|---|---|---|---|".to_string());
        } else {
            lines.push("| ID | Question | Answer |".to_string());
            lines.push("|---|---|---|".to_string());
        }
        recs.sort_by(|a, b| a.id.cmp(&b.id));
        for rec in &recs {
            if section_has_aliases {
                let aliases = if rec.aliases.is_empty() {
                    String::new()
                } else {
                    format!("also: {}", rec.aliases.join("; "))
                };
                lines.push(format!(
                    "| {} | {} | {} | {} |",
                    rec.id, rec.question, rec.answer, aliases
                ));
            } else {
                lines.push(format!(
                    "| {} | {} | {} |",
                    rec.id, rec.question, rec.answer
                ));
            }
            count += 1;
            let hash = content_hash(&rec.question, &rec.answer);
            commit_record(
                store,
                rec,
                import_provenance(&bank_path.display().to_string(), &hash, rec.version),
                &format!("question export sync {}/{}", rec.repo, rec.id),
            )?;
        }
        lines.push(String::new());
    }

    std::fs::write(bank_path, lines.join("\n")).map_err(|e| {
        CodeintelError::Other(format!(
            "cannot write question bank {}: {e}",
            bank_path.display()
        ))
    })?;
    Ok(count)
}

/// Parse the markdown table rows from the question bank content.
fn parse_question_bank(content: &str) -> Vec<QuestionRecord> {
    let mut records = Vec::new();
    let mut current_repo = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("## ") {
            let section = trimmed.trim_start_matches("## ").trim().to_lowercase();
            current_repo = if section.contains("cross") {
                String::new()
            } else {
                section.replace(' ', "-")
            };
            continue;
        }

        if !trimmed.starts_with('|') {
            continue;
        }

        let cells: Vec<&str> = trimmed.split('|').map(|c| c.trim()).collect();
        if cells.len() < 5 {
            continue;
        }

        let id = cells[1];
        let question = cells[2];
        let answer = cells[3];
        let aliases = if cells.len() >= 6 && !cells[4].is_empty() {
            parse_aliases_cell(cells[4])
        } else {
            Vec::new()
        };

        if id.is_empty()
            || id == "#"
            || id == "ID"
            || id.starts_with('-')
            || question.starts_with('-')
        {
            continue;
        }
        if question.is_empty() || answer.is_empty() {
            continue;
        }

        let tags = compute_tags(question, answer, &aliases);

        records.push(QuestionRecord {
            id: id.to_string(),
            repo: current_repo.clone(),
            question: question.to_string(),
            answer: answer.to_string(),
            tags,
            status: QuestionStatus::Active,
            version: 1,
            aliases,
        });
    }

    records
}

fn parse_aliases_cell(cell: &str) -> Vec<String> {
    let mut s = cell.trim();
    if let Some(rest) = s.strip_prefix("also:") {
        s = rest.trim();
    }
    s.split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn compute_tags(question: &str, answer: &str, aliases: &[String]) -> Vec<String> {
    let mut combined = format!("{question} {answer}");
    for alias in aliases {
        combined.push(' ');
        combined.push_str(alias);
    }
    let combined = combined.to_lowercase();
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
    use crate::model::QuestionStatus;

    #[test]
    fn content_hash_normalizes_whitespace() {
        let a = content_hash("What is X?", "Answer   one.");
        let b = content_hash("What  is X?", "Answer one.");
        assert_eq!(a, b);
    }

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
    fn learn_question_is_idempotent_until_the_answer_changes() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let q = "Where are nodeport hostport hyperdx documented?";
        let a = "dpk-infra documents this in docs/setup.md. Do not search a second repo.";
        let first = learn_question(&store, "dpk-infra", q, a).unwrap();
        assert!(first.id.starts_with("L-"));
        assert_eq!(first.version, 1);
        let again = learn_question(&store, "dpk-infra", q, a).unwrap();
        assert_eq!(again.version, 1);
        assert_eq!(again.id, first.id);
        let updated = learn_question(
            &store,
            "dpk-infra",
            q,
            "dpk-infra documents this in docs/other.md. Do not search a second repo.",
        )
        .unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(
            learned_question_id(q),
            learned_question_id("  Where   are nodeport hostport hyperdx documented?  ")
        );
    }

    #[test]
    fn patch_requires_reason_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        put_question(&store, "codeintel", "T98", "Q?", "A.").unwrap();
        let err = patch_question_with_opts(
            &store,
            "codeintel",
            "T98",
            Some("B."),
            None,
            QuestionWriteOpts {
                reason: Some("correction"),
                expected_version: Some(99),
            },
        )
        .unwrap_err();
        assert!(matches!(err, CodeintelError::StaleVersion { .. }));
    }

    #[test]
    fn parses_optional_aliases_column() {
        let md = "## codeintel\n\
                  | ID | Question | Answer | Aliases |\n\
                  |---|---|---|---|\n\
                  | C7 | Canonical? | Answer text. | also: paraphrase one; paraphrase two |\n";
        let recs = parse_question_bank(md);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].aliases.len(), 2);
        assert_eq!(recs[0].aliases[0], "paraphrase one");
    }

    #[test]
    fn patch_appends_aliases_without_answer() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let created = put_question(&store, "codeintel", "T99", "Q?", "A.").unwrap();
        let updated = patch_question_with_opts(
            &store,
            "codeintel",
            "T99",
            None,
            Some(&["also phrasing".into()]),
            QuestionWriteOpts {
                reason: Some("enrichment"),
                expected_version: Some(created.version),
            },
        )
        .unwrap();
        assert_eq!(updated.aliases, vec!["also phrasing"]);
        assert_eq!(updated.answer, "A.");
    }

    #[test]
    fn patch_question_answer_with_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let created = put_question(&store, "codeintel", "T98", "Original?", "Old.").unwrap();
        let updated = patch_question_with_opts(
            &store,
            "codeintel",
            "T98",
            Some("New authoritative answer."),
            None,
            QuestionWriteOpts {
                reason: Some("correction"),
                expected_version: Some(created.version),
            },
        )
        .unwrap();
        assert_eq!(updated.question, "Original?");
        assert_eq!(updated.answer, "New authoritative answer.");
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn import_hash_match_syncs_provenance_without_version_bump() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        put_question_with_opts(
            &store,
            "codeintel",
            "T1",
            "Q?",
            "Answer.",
            QuestionWriteOpts {
                reason: Some("create"),
                expected_version: None,
            },
        )
        .unwrap();
        let bank = dir.path().join("bank.md");
        let md = "## codeintel\n| ID | Question | Answer |\n|---|---|---|\n| T1 | Q? | Answer. |\n";
        std::fs::write(&bank, md).unwrap();
        let report = index_questions_with_opts(&store, &bank, false).unwrap();
        assert_eq!(report.synced, 1);
        let rec = get_question(&store, "codeintel", "T1").unwrap().unwrap();
        assert_eq!(rec.version, 1);
    }

    #[test]
    fn delete_is_irreversible() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let rec = put_question(&store, "codeintel", "D1", "Q?", "A.").unwrap();
        delete_question(
            &store,
            "codeintel",
            "D1",
            QuestionWriteOpts {
                reason: Some("delete"),
                expected_version: Some(rec.version),
            },
        )
        .unwrap();
        let err = reactivate_question(
            &store,
            "codeintel",
            "D1",
            QuestionWriteOpts {
                reason: Some("reactivate"),
                expected_version: Some(2),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("deleted"));
    }

    #[test]
    fn retired_record_stays_in_store_but_inactive() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let rec = put_question(
            &store,
            "codeintel",
            "G1",
            "What is bindb boundary?",
            "codeintel writes bindb only.",
        )
        .unwrap();
        retire_question(
            &store,
            "codeintel",
            "G1",
            QuestionWriteOpts {
                reason: Some("retire"),
                expected_version: Some(rec.version),
            },
        )
        .unwrap();
        let loaded = get_question(&store, "codeintel", "G1").unwrap().unwrap();
        assert_eq!(loaded.status, QuestionStatus::Retired);
    }

    #[test]
    fn bank_import_reactivates_retired_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let bank = dir.path().join("bank.md");
        put_question(
            &store,
            "codeintel",
            "G1",
            "What is bindb boundary?",
            "codeintel writes bindb only.",
        )
        .unwrap();
        let retired = retire_question(
            &store,
            "codeintel",
            "G1",
            QuestionWriteOpts {
                reason: Some("retire"),
                expected_version: Some(1),
            },
        )
        .unwrap();
        assert_eq!(retired.status, QuestionStatus::Retired);
        std::fs::write(
            &bank,
            "## codeintel\n| ID | Question | Answer |\n|---|---|---|\n\
             | G1 | What is bindb boundary? | codeintel writes bindb only. |\n",
        )
        .unwrap();
        let report = index_questions_with_opts(&store, &bank, false).unwrap();
        assert_eq!(report.rows_active, 1);
        assert_eq!(report.conflicts, 0);
        let loaded = get_question(&store, "codeintel", "G1").unwrap().unwrap();
        assert_eq!(loaded.status, QuestionStatus::Active);
    }

    #[test]
    fn bank_import_syncs_aliases_when_content_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let bank = dir.path().join("bank.md");
        put_question(
            &store,
            "codeintel",
            "C7",
            "When PlanReport changes?",
            "propagate field.",
        )
        .unwrap();
        std::fs::write(
            &bank,
            "## codeintel\n| ID | Question | Answer | Aliases |\n|---|---|---|---|\n\
             | C7 | When PlanReport changes? | propagate field. | also: paraphrase one |\n",
        )
        .unwrap();
        let report = index_questions_with_opts(&store, &bank, false).unwrap();
        assert_eq!(report.rows_active, 1);
        assert_eq!(report.indexed, 1);
        let loaded = get_question(&store, "codeintel", "C7").unwrap().unwrap();
        assert_eq!(loaded.aliases, vec!["paraphrase one"]);
    }

    #[test]
    fn bank_import_reactivates_deleted_row() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let bank = dir.path().join("bank.md");
        let created = put_question(
            &store,
            "codeintel",
            "G2",
            "What is bindb boundary?",
            "codeintel writes bindb only.",
        )
        .unwrap();
        delete_question(
            &store,
            "codeintel",
            "G2",
            QuestionWriteOpts {
                reason: Some("delete"),
                expected_version: Some(created.version),
            },
        )
        .unwrap();
        std::fs::write(
            &bank,
            "## codeintel\n| ID | Question | Answer |\n|---|---|---|\n\
             | G2 | What is bindb boundary? | codeintel writes bindb only. |\n",
        )
        .unwrap();
        let report = index_questions_with_opts(&store, &bank, false).unwrap();
        assert_eq!(report.rows_active, 1);
        assert_eq!(report.conflicts, 0);
        let loaded = get_question(&store, "codeintel", "G2").unwrap().unwrap();
        assert_eq!(loaded.status, QuestionStatus::Active);
        assert_eq!(loaded.answer, "codeintel writes bindb only.");
    }

    #[test]
    fn bank_import_updates_lifecycle_row_when_no_import_anchor() {
        let dir = tempfile::tempdir().unwrap();
        let store = bindb::BinderStore::open(dir.path()).unwrap();
        let bank = dir.path().join("bank.md");
        put_question(
            &store,
            "repo-dashboard",
            "R14",
            "How are docs used?",
            "Lifecycle answer from bootstrap.",
        )
        .unwrap();
        std::fs::write(
            &bank,
            "## repo-dashboard\n| ID | Question | Answer |\n|---|---|---|\n\
             | R14 | How are docs used? | Git bank is source of truth on refresh. |\n",
        )
        .unwrap();
        let report = index_questions_with_opts(&store, &bank, false).unwrap();
        assert_eq!(report.rows_active, 1);
        assert_eq!(report.conflicts, 0);
        let loaded = get_question(&store, "repo-dashboard", "R14")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.answer, "Git bank is source of truth on refresh.");
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
        assert!(all
            .iter()
            .any(|r| r.id == "T99" && r.answer.contains("bindb")));
    }
}
