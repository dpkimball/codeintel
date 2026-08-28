//! Autonomous engineering workflow: bindb plans, source is opened only to implement or verify.

use crate::architecture::{Architecture, ImpactView};
use crate::error::{CodeintelError, Result};
use crate::indexer::{index, open_store, IndexOptions, IndexStats};
use crate::query::QueryEngine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoBind {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctrineSnapshot {
    pub repository: String,
    pub doctrine: String,
    pub findings: usize,
    pub rules_evaluated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactSnapshot {
    pub target: String,
    pub target_kind: String,
    pub files: Vec<String>,
    pub transitive_crate_dependents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSession {
    pub request: String,
    pub store: PathBuf,
    pub binds: Vec<RepoBind>,
    pub planning_ms: u64,
    pub interpretation: String,
    pub matched_kind: String,
    pub repositories_consulted: Vec<String>,
    pub files_identified: Vec<String>,
    pub graph_files: Vec<String>,
    pub graph_file_count: usize,
    pub likely_affected: Vec<String>,
    pub rules_must_remain_satisfied: Vec<String>,
    pub doctrine_before: Vec<DoctrineSnapshot>,
    pub impact_before: Option<ImpactSnapshot>,
    pub impact_target: Option<String>,
    pub source_files_opened_during_planning: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRecord {
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowReport {
    pub request: String,
    pub planning_ms: u64,
    pub reindex_ms: u64,
    pub repositories_consulted: usize,
    pub repositories: Vec<String>,
    pub files_identified_by_graph: usize,
    pub files_identified: Vec<String>,
    pub graph_file_count: usize,
    pub files_opened: usize,
    pub files_opened_list: Vec<String>,
    pub files_modified: usize,
    pub files_modified_list: Vec<String>,
    pub files_new: Vec<String>,
    pub opened_outside_inspection_set: Vec<String>,
    pub validation: ValidationRecord,
    pub doctrine_before: Vec<DoctrineSnapshot>,
    pub doctrine_after: Vec<DoctrineSnapshot>,
    pub doctrine_status_before: String,
    pub doctrine_status_after: String,
    pub impact_before: Option<ImpactSnapshot>,
    pub impact_after: Option<ImpactSnapshot>,
    pub reindex: Vec<IndexStats>,
    pub primary_source: &'static str,
    pub source_files_opened_during_graph_phases: usize,
}

/// Parse `--bind name=/path/to/repo`.
pub fn parse_bind(spec: &str) -> Result<RepoBind> {
    let Some((name, path)) = spec.split_once('=') else {
        return Err(CodeintelError::Other(
            "bind must be name=path (example: codeintel=./codeintel)".into(),
        ));
    };
    let name = name.trim();
    let path = path.trim();
    if name.is_empty() || path.is_empty() {
        return Err(CodeintelError::Other(
            "bind name and path must be non-empty".into(),
        ));
    }
    Ok(RepoBind {
        name: name.to_string(),
        path: PathBuf::from(path),
    })
}

/// Open the store, plan from the graph, snapshot doctrine/impact. Does not read source trees.
pub fn start(
    store: impl AsRef<Path>,
    request: &str,
    binds: Vec<RepoBind>,
    session_path: impl AsRef<Path>,
) -> Result<WorkflowSession> {
    let store = store.as_ref().to_path_buf();
    let arch = Architecture::open(&store)?;
    let planning = Instant::now();
    let plan = arch.plan(request)?;
    let planning_ms = planning.elapsed().as_millis() as u64;
    if plan.source_files_opened != 0 {
        return Err(CodeintelError::Other(
            "plan opened source files; the workflow requires graph-only planning".into(),
        ));
    }
    let doctrine = arch.doctrine()?;
    let doctrine_before = snapshots_from_doctrine(&doctrine);
    let impact_target = plan
        .smallest_file_set
        .iter()
        .find(|p| p.ends_with(".rs"))
        .cloned()
        .or_else(|| plan.smallest_file_set.first().cloned())
        .or_else(|| {
            plan.likely_affected
                .first()
                .and_then(|s| s.split('/').next_back().map(|x| x.to_string()))
        });
    let impact_before = match &impact_target {
        Some(target) => Some(snapshot_impact(&arch.impact(target)?)),
        None => None,
    };
    let source_files_opened_during_planning = plan.source_files_opened;
    arch.close()?;

    let graph_files = indexed_file_paths(&store)?;
    let session = WorkflowSession {
        request: request.to_string(),
        store,
        binds,
        planning_ms,
        interpretation: plan.interpretation,
        matched_kind: plan.matched_kind,
        repositories_consulted: plan.repositories,
        files_identified: plan.smallest_file_set,
        graph_file_count: graph_files.len(),
        graph_files,
        likely_affected: plan.likely_affected,
        rules_must_remain_satisfied: plan.rules_must_remain_satisfied,
        doctrine_before,
        impact_before,
        impact_target,
        source_files_opened_during_planning,
    };
    write_session(session_path.as_ref(), &session)?;
    Ok(session)
}

/// Re-index bound repos, re-evaluate doctrine, emit metrics. Graph phases do not walk source.
pub fn finish(
    session_path: impl AsRef<Path>,
    opened: Vec<String>,
    modified: Vec<String>,
    validation: ValidationRecord,
) -> Result<WorkflowReport> {
    let session = read_session(session_path.as_ref())?;
    let identified: BTreeSet<String> = session.files_identified.iter().cloned().collect();
    let graph_files: BTreeSet<String> = session.graph_files.iter().cloned().collect();
    let mut files_new = Vec::new();
    let mut opened_outside = Vec::new();
    for path in &opened {
        if identified.contains(path) {
            continue;
        }
        if graph_files.contains(path) {
            opened_outside.push(path.clone());
        } else {
            files_new.push(path.clone());
        }
    }

    let reindex_started = Instant::now();
    let mut reindex = Vec::new();
    for bind in &session.binds {
        reindex.push(index(IndexOptions {
            repo_root: bind.path.clone(),
            store_path: session.store.clone(),
            name: Some(bind.name.clone()),
        })?);
    }
    let reindex_ms = reindex_started.elapsed().as_millis() as u64;

    let arch = Architecture::open(&session.store)?;
    let doctrine = arch.doctrine()?;
    let doctrine_after = snapshots_from_doctrine(&doctrine);
    let impact_after = match &session.impact_target {
        Some(target) => Some(snapshot_impact(&arch.impact(target)?)),
        None => None,
    };
    let graph_opened = doctrine.source_files_opened + session.source_files_opened_during_planning;
    arch.close()?;

    Ok(WorkflowReport {
        request: session.request,
        planning_ms: session.planning_ms,
        reindex_ms,
        repositories_consulted: session.repositories_consulted.len(),
        repositories: session.repositories_consulted,
        files_identified_by_graph: session.files_identified.len(),
        files_identified: session.files_identified,
        graph_file_count: session.graph_file_count,
        files_opened: opened.len(),
        files_opened_list: opened,
        files_modified: modified.len(),
        files_modified_list: modified,
        files_new,
        opened_outside_inspection_set: opened_outside,
        validation,
        doctrine_status_before: status_of(&session.doctrine_before),
        doctrine_status_after: status_of(&doctrine_after),
        doctrine_before: session.doctrine_before,
        doctrine_after,
        impact_before: session.impact_before,
        impact_after,
        reindex,
        primary_source: "bindb",
        source_files_opened_during_graph_phases: graph_opened,
    })
}

pub fn write_session(path: &Path, session: &WorkflowSession) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, serde_json::to_vec_pretty(session)?)?;
    Ok(())
}

pub fn read_session(path: &Path) -> Result<WorkflowSession> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn indexed_file_paths(store: &Path) -> Result<Vec<String>> {
    let db = open_store(store)?;
    let mut files = BTreeSet::new();
    for v in db.list(crate::ids::PREFIX_ALL_REPOS)? {
        let rec: crate::model::RepoRecord = crate::indexer::decode_obj(&v)?;
        let q = QueryEngine::open(&db, &rec.name);
        for f in q.files()? {
            files.insert(f.path);
        }
    }
    db.close()?;
    Ok(files.into_iter().collect())
}

fn snapshots_from_doctrine(view: &crate::architecture::DoctrineView) -> Vec<DoctrineSnapshot> {
    view.repositories
        .iter()
        .map(|r| DoctrineSnapshot {
            repository: r.repository.clone(),
            doctrine: r.report.doctrine.clone(),
            findings: r.report.findings.len(),
            rules_evaluated: r.report.rules_evaluated,
        })
        .collect()
}

fn snapshot_impact(view: &ImpactView) -> ImpactSnapshot {
    ImpactSnapshot {
        target: view.report.target.clone(),
        target_kind: format!("{:?}", view.report.target_kind).to_lowercase(),
        files: view.report.inspection_set.files.clone(),
        transitive_crate_dependents: view.report.transitive_crate_dependents.clone(),
    }
}

fn status_of(snaps: &[DoctrineSnapshot]) -> String {
    let findings: usize = snaps.iter().map(|s| s.findings).sum();
    if findings == 0 {
        "satisfied".into()
    } else {
        format!("{findings} finding(s)")
    }
}
