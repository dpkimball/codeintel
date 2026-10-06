//! Federated graph across repositories in one bindb store.

use crate::error::{CodeintelError, Result};
use crate::ids::{
    edge_id, file_id, parse_crate_id, ACTOR, PREFIX_ALL_CRATES, PREFIX_ALL_REPOS, REL_DEPENDS_ON,
};
use crate::indexer::decode_obj;
use crate::model::{CrateRecord, FileRecord, RepoRecord};
use crate::query::crate_cycles;
use bindb::{BinderStore, ObjectId, Provenance, ProvenanceSource, Transaction};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct CrossRepoDep {
    pub from_repo: String,
    pub from_crate: String,
    pub to_repo: String,
    pub to_crate: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FederatedCrate {
    pub repo: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FederationReport {
    pub repos: Vec<String>,
    pub crates: Vec<FederatedCrate>,
    pub cross_repo_deps: Vec<CrossRepoDep>,
    pub cycles: Vec<Vec<String>>,
    pub linked_this_pass: u64,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathFile {
    pub path: String,
    pub crate_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathImpact {
    pub repo: String,
    pub files: Vec<PathFile>,
    pub unmapped_paths: Vec<String>,
    /// Crates in other repositories that `DEPENDS_ON` a crate touched by `files`.
    pub dependents: Vec<FederatedCrate>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FederationImpact {
    pub target_repo: String,
    pub target_crate: String,
    pub object_id: String,
    pub dependents: Vec<FederatedCrate>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

struct IndexedCrate {
    repo: String,
    name: String,
    id: ObjectId,
    dependencies: Vec<String>,
}

/// Write `DEPENDS_ON` when a crate's declared dep name matches a crate in another repo.
pub fn link_cross_repo(store: &BinderStore) -> Result<u64> {
    let crates = load_crates(store)?;
    let mut by_name: HashMap<String, Vec<&IndexedCrate>> = HashMap::new();
    for c in &crates {
        by_name.entry(c.name.clone()).or_default().push(c);
    }
    let provenance = Provenance {
        source: ProvenanceSource::Import("federation".into()),
        source_ref: "cross-repo".into(),
        imported_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        ..Default::default()
    };
    let mut tx = Transaction::new();
    let mut linked = 0u64;
    for from in &crates {
        let existing: HashSet<String> = store
            .edges_from_with_type(&from.id, REL_DEPENDS_ON)?
            .into_iter()
            .filter_map(|e| e.to_object.map(|o| o.0))
            .collect();
        for dep in &from.dependencies {
            let Some(candidates) = by_name.get(dep) else {
                continue;
            };
            for to in candidates {
                if to.repo == from.repo {
                    continue;
                }
                if existing.contains(&to.id.0) {
                    continue;
                }
                let eid = edge_id(REL_DEPENDS_ON, &from.id, &to.id);
                tx.put_edge_with_provenance(
                    eid,
                    from.id.clone(),
                    to.id.clone(),
                    REL_DEPENDS_ON,
                    None,
                    Some(provenance.clone()),
                );
                linked += 1;
            }
        }
    }
    if linked > 0 {
        store.commit_buffered(tx, ACTOR, "federation cross-repo deps")?;
        store.flush()?;
    }
    Ok(linked)
}

pub fn analyze(store: &BinderStore) -> Result<FederationReport> {
    let linked_this_pass = link_cross_repo(store)?;
    let repos = load_repos(store)?;
    let crates = load_crates(store)?;
    let mut cross_repo_deps = Vec::new();
    let mut graph: Vec<(String, String)> = Vec::new();
    for c in &crates {
        let from_label = format!("{}/{}", c.repo, c.name);
        for e in store.edges_from_with_type(&c.id, REL_DEPENDS_ON)? {
            let Some(to_id) = e.to_object else {
                continue;
            };
            let Some((to_repo, to_crate)) = parse_crate_id(&to_id) else {
                continue;
            };
            let to_label = format!("{to_repo}/{to_crate}");
            graph.push((from_label.clone(), to_label));
            if to_repo != c.repo {
                cross_repo_deps.push(CrossRepoDep {
                    from_repo: c.repo.clone(),
                    from_crate: c.name.clone(),
                    to_repo,
                    to_crate,
                });
            }
        }
    }
    cross_repo_deps.sort_by(|a, b| {
        (&a.from_repo, &a.from_crate, &a.to_repo, &a.to_crate).cmp(&(
            &b.from_repo,
            &b.from_crate,
            &b.to_repo,
            &b.to_crate,
        ))
    });
    let cycles = crate_cycles(&graph)
        .into_iter()
        .map(|c| c.nodes)
        .filter(|nodes| spans_repos(nodes))
        .collect();
    Ok(FederationReport {
        repos,
        crates: crates
            .into_iter()
            .map(|c| FederatedCrate {
                repo: c.repo,
                name: c.name,
            })
            .collect(),
        cross_repo_deps,
        cycles,
        linked_this_pass,
        primary_source: "bindb",
        source_files_opened: 0,
    })
}

pub fn impact(store: &BinderStore, spec: &str) -> Result<FederationImpact> {
    let _ = link_cross_repo(store)?;
    impact_existing(store, spec)
}

/// Cross-repo dependents of the crates that own `paths`. Reads file objects already
/// in the store. Does not write federation edges.
pub fn impact_paths(store: &BinderStore, repo: &str, paths: &[String]) -> Result<PathImpact> {
    if paths.is_empty() {
        return Err(CodeintelError::Other("no paths".into()));
    }
    let mut files = Vec::new();
    let mut unmapped_paths = Vec::new();
    let mut crate_names = Vec::new();
    for path in paths {
        let path = normalize_repo_path(path)?;
        let Some(raw) = store.get(&file_id(repo, &path))? else {
            unmapped_paths.push(path);
            continue;
        };
        let rec: FileRecord = decode_obj(&raw)?;
        match rec.crate_name.as_deref().filter(|n| !n.is_empty()) {
            Some(crate_name) => {
                if !crate_names.iter().any(|n| n == crate_name) {
                    crate_names.push(crate_name.to_string());
                }
                files.push(PathFile {
                    path,
                    crate_name: crate_name.to_string(),
                });
            }
            None => unmapped_paths.push(path),
        }
    }
    let mut dependents = Vec::new();
    let mut seen = HashSet::new();
    for crate_name in &crate_names {
        let spec = format!("{repo}/{crate_name}");
        let impact = impact_existing(store, &spec)?;
        for dep in impact.dependents {
            if dep.repo == repo {
                continue;
            }
            if seen.insert((dep.repo.clone(), dep.name.clone())) {
                dependents.push(dep);
            }
        }
    }
    dependents.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.name.cmp(&b.name)));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    unmapped_paths.sort();
    Ok(PathImpact {
        repo: repo.to_string(),
        files,
        unmapped_paths,
        dependents,
        primary_source: "bindb",
        source_files_opened: 0,
    })
}

fn impact_existing(store: &BinderStore, spec: &str) -> Result<FederationImpact> {
    let crates = load_crates(store)?;
    let target = resolve_crate(&crates, spec)?;
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for c in &crates {
        for e in store.edges_from_with_type(&c.id, REL_DEPENDS_ON)? {
            let Some(to_id) = e.to_object else {
                continue;
            };
            adj.entry(to_id.0).or_default().push(c.id.0.clone());
        }
    }
    let mut dependents = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![target.id.0.clone()];
    seen.insert(target.id.0.clone());
    while let Some(id) = stack.pop() {
        let Some(incoming) = adj.get(&id) else {
            continue;
        };
        for from in incoming {
            if !seen.insert(from.clone()) {
                continue;
            }
            stack.push(from.clone());
            if let Some(c) = crates.iter().find(|c| c.id.0 == *from) {
                if c.id.0 != target.id.0 {
                    dependents.push(FederatedCrate {
                        repo: c.repo.clone(),
                        name: c.name.clone(),
                    });
                }
            }
        }
    }
    dependents.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.name.cmp(&b.name)));
    Ok(FederationImpact {
        target_repo: target.repo.clone(),
        target_crate: target.name.clone(),
        object_id: target.id.0.clone(),
        dependents,
        primary_source: "bindb",
        source_files_opened: 0,
    })
}

fn normalize_repo_path(path: &str) -> Result<String> {
    let path = path.trim().trim_start_matches("./");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path
            .split('/')
            .any(|seg| seg.is_empty() || seg == ".." || seg == ".")
    {
        return Err(CodeintelError::Other(format!("invalid path {path}")));
    }
    Ok(path.to_string())
}

fn spans_repos(nodes: &[String]) -> bool {
    let repos: HashSet<&str> = nodes.iter().filter_map(|n| n.split('/').next()).collect();
    repos.len() > 1
}

fn resolve_crate<'a>(crates: &'a [IndexedCrate], spec: &str) -> Result<&'a IndexedCrate> {
    let spec = spec.trim();
    if let Some((repo, name)) = spec.split_once('/') {
        if let Some(c) = crates.iter().find(|c| c.repo == repo && c.name == name) {
            return Ok(c);
        }
    }
    let hits: Vec<&IndexedCrate> = crates.iter().filter(|c| c.name == spec).collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(CodeintelError::Other(format!(
            "crate {spec} is not in the federated graph"
        ))),
        n => Err(CodeintelError::Other(format!(
            "crate {spec} is ambiguous ({n} repos); use repo/crate"
        ))),
    }
}

fn load_repos(store: &BinderStore) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for v in store.list(PREFIX_ALL_REPOS)? {
        let rec: RepoRecord = decode_obj(&v)?;
        names.push(rec.name);
    }
    names.sort();
    Ok(names)
}

fn load_crates(store: &BinderStore) -> Result<Vec<IndexedCrate>> {
    let mut out = Vec::new();
    for v in store.list(PREFIX_ALL_CRATES)? {
        let rec: CrateRecord = decode_obj(&v)?;
        let Some((parsed_repo, parsed_name)) = parse_crate_id(&v.object_id) else {
            continue;
        };
        out.push(IndexedCrate {
            repo: if rec.repository.is_empty() {
                parsed_repo
            } else {
                rec.repository
            },
            name: if rec.name.is_empty() {
                parsed_name
            } else {
                rec.name
            },
            id: v.object_id,
            dependencies: rec.dependencies,
        });
    }
    Ok(out)
}
