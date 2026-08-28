//! Historical architectural drift: compare two indexed Git SHAs via bindb as_of.

use crate::doctrine::{Doctrine, Finding};
use crate::error::{CodeintelError, Result};
use crate::ids::{crate_id, edge_id, prefix_commit, PREFIX_ALL_COMMITS, REL_DEPENDS_ON};
use crate::indexer::decode_obj;
use crate::model::CommitRecord;
use bindb::{BinderStore, CommitId};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize)]
pub struct DriftReport {
    pub from_sha: String,
    pub to_sha: String,
    pub from_bindb_commit: String,
    pub to_bindb_commit: String,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
    pub crates_added: Vec<String>,
    pub crates_removed: Vec<String>,
    pub files_added: Vec<String>,
    pub files_removed: Vec<String>,
    pub deps_added: Vec<(String, String)>,
    pub deps_removed: Vec<(String, String)>,
    pub doctrine_introduced: Vec<Finding>,
    pub doctrine_resolved: Vec<Finding>,
    pub symbols_added: Vec<String>,
    pub symbols_removed: Vec<String>,
}

pub fn analyze(
    store: &BinderStore,
    name: &str,
    from_sha: &str,
    to_sha: &str,
) -> Result<DriftReport> {
    let from = load_commit(store, name, from_sha)?;
    let to = load_commit(store, name, to_sha)?;
    let from_cid = require_bindb(&from)?;
    let to_cid = require_bindb(&to)?;

    let crates_from = crates_at(store, name, from_cid, &from.crate_names)?;
    let crates_to = crates_at(store, name, to_cid, &to.crate_names)?;
    let (crates_added, crates_removed) = set_diff(&crates_from, &crates_to);

    let files_from = files_at(store, name, from_cid, &from.file_paths)?;
    let files_to = files_at(store, name, to_cid, &to.file_paths)?;
    let (files_added, files_removed) = set_diff(&files_from, &files_to);

    let union_crates: Vec<String> = crates_from
        .iter()
        .chain(crates_to.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let deps_from = deps_at(store, name, from_cid, &union_crates)?;
    let deps_to = deps_at(store, name, to_cid, &union_crates)?;
    let (deps_added, deps_removed) = pair_diff(&deps_from, &deps_to);

    let doctrine = Doctrine::detect(&union_crates);
    let findings_from = doctrine.evaluate_graph(name, &deps_from)?.findings;
    let findings_to = doctrine.evaluate_graph(name, &deps_to)?.findings;
    let (doctrine_introduced, doctrine_resolved) = finding_diff(&findings_from, &findings_to);

    let symbols_from = symbols_at(store, name, from_cid, &from.symbol_qualnames)?;
    let symbols_to = symbols_at(store, name, to_cid, &to.symbol_qualnames)?;
    let (symbols_added, symbols_removed) = set_diff(&symbols_from, &symbols_to);

    Ok(DriftReport {
        from_sha: from.sha,
        to_sha: to.sha,
        from_bindb_commit: from_cid.to_string(),
        to_bindb_commit: to_cid.to_string(),
        primary_source: "bindb",
        source_files_opened: 0,
        crates_added,
        crates_removed,
        files_added,
        files_removed,
        deps_added,
        deps_removed,
        doctrine_introduced,
        doctrine_resolved,
        symbols_added,
        symbols_removed,
    })
}

/// Compare two indexed Git SHAs; repo is inferred from commit catalogs.
pub fn analyze_indexed(store: &BinderStore, from_sha: &str, to_sha: &str) -> Result<DriftReport> {
    let from = find_commit(store, from_sha)?;
    let to = find_commit(store, to_sha)?;
    if from.repository != to.repository {
        return Err(CodeintelError::Other(format!(
            "SHAs belong to different repositories ({} vs {}); drift is per-repo",
            from.repository, to.repository
        )));
    }
    analyze(store, &from.repository, &from.sha, &to.sha)
}

fn find_commit(store: &BinderStore, spec: &str) -> Result<CommitRecord> {
    let mut matches = Vec::new();
    for v in store.list(PREFIX_ALL_COMMITS)? {
        let rec: CommitRecord = decode_obj(&v)?;
        if rec.sha == spec || rec.sha.starts_with(spec) {
            matches.push(rec);
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(CodeintelError::Other(format!(
            "git SHA {spec} is not indexed; run `codeintel index` at that commit first"
        ))),
        n => Err(CodeintelError::Other(format!(
            "git SHA prefix {spec} is ambiguous ({n} indexed matches)"
        ))),
    }
}

fn load_commit(store: &BinderStore, name: &str, spec: &str) -> Result<CommitRecord> {
    let exact = crate::ids::commit_id(name, spec);
    if let Some(v) = store.get(&exact)? {
        return decode_obj(&v);
    }
    let prefix = prefix_commit(name);
    let mut matches = Vec::new();
    for v in store.list(&prefix)? {
        let rec: CommitRecord = decode_obj(&v)?;
        if rec.sha == spec || rec.sha.starts_with(spec) {
            matches.push(rec);
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(CodeintelError::Other(format!(
            "git SHA {spec} is not indexed; run `codeintel index` at that commit first"
        ))),
        n => Err(CodeintelError::Other(format!(
            "git SHA prefix {spec} is ambiguous ({n} indexed matches)"
        ))),
    }
}

fn require_bindb(rec: &CommitRecord) -> Result<CommitId> {
    rec.bindb_commit.ok_or_else(|| {
        CodeintelError::Other(format!(
            "commit {} has no bindb_commit catalog; re-index that SHA",
            rec.sha
        ))
    })
}

fn crates_at(
    store: &BinderStore,
    name: &str,
    at: CommitId,
    catalog: &[String],
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for c in catalog {
        if store.object_as_of(&crate_id(name, c), at)?.is_some() {
            out.push(c.clone());
        }
    }
    Ok(out)
}

fn files_at(
    store: &BinderStore,
    name: &str,
    at: CommitId,
    catalog: &[String],
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for path in catalog {
        if store
            .object_as_of(&crate::ids::file_id(name, path), at)?
            .is_some()
        {
            out.push(path.clone());
        }
    }
    Ok(out)
}

fn symbols_at(
    store: &BinderStore,
    name: &str,
    at: CommitId,
    catalog: &[String],
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for qual in catalog {
        if store
            .object_as_of(&crate::ids::symbol_id(name, qual), at)?
            .is_some()
        {
            out.push(qual.clone());
        }
    }
    Ok(out)
}

fn deps_at(
    store: &BinderStore,
    name: &str,
    at: CommitId,
    crates: &[String],
) -> Result<Vec<(String, String)>> {
    let mut edges = Vec::new();
    for from in crates {
        for to in crates {
            if from == to {
                continue;
            }
            let eid = edge_id(REL_DEPENDS_ON, &crate_id(name, from), &crate_id(name, to));
            if let Some(e) = store.edge_as_of(&eid, at)? {
                if !e.is_tombstone() {
                    edges.push((from.clone(), to.clone()));
                }
            }
        }
    }
    Ok(edges)
}

fn set_diff(from: &[String], to: &[String]) -> (Vec<String>, Vec<String>) {
    let a: BTreeSet<&str> = from.iter().map(String::as_str).collect();
    let b: BTreeSet<&str> = to.iter().map(String::as_str).collect();
    let added: Vec<String> = b.difference(&a).map(|s| (*s).to_string()).collect();
    let removed: Vec<String> = a.difference(&b).map(|s| (*s).to_string()).collect();
    (added, removed)
}

type DepEdge = (String, String);

fn pair_diff(from: &[DepEdge], to: &[DepEdge]) -> (Vec<DepEdge>, Vec<DepEdge>) {
    let a: BTreeSet<(&str, &str)> = from.iter().map(|(x, y)| (x.as_str(), y.as_str())).collect();
    let b: BTreeSet<(&str, &str)> = to.iter().map(|(x, y)| (x.as_str(), y.as_str())).collect();
    let added = b
        .difference(&a)
        .map(|(x, y)| ((*x).to_string(), (*y).to_string()))
        .collect();
    let removed = a
        .difference(&b)
        .map(|(x, y)| ((*x).to_string(), (*y).to_string()))
        .collect();
    (added, removed)
}

fn finding_key(f: &Finding) -> String {
    format!(
        "{}|{}|{}|{}",
        f.rule_id, f.evidence.from, f.evidence.to, f.evidence.detail
    )
}

fn finding_diff(from: &[Finding], to: &[Finding]) -> (Vec<Finding>, Vec<Finding>) {
    let from_keys: BTreeSet<String> = from.iter().map(finding_key).collect();
    let to_keys: BTreeSet<String> = to.iter().map(finding_key).collect();
    let introduced = to
        .iter()
        .filter(|f| !from_keys.contains(&finding_key(f)))
        .cloned()
        .collect();
    let resolved = from
        .iter()
        .filter(|f| !to_keys.contains(&finding_key(f)))
        .cloned()
        .collect();
    (introduced, resolved)
}
