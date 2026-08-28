//! Agent-facing architecture API. Operations are semantic; bindb stays behind this type.

use crate::doctrine::{ConformanceReport, Doctrine};
use crate::drift::{analyze_indexed, DriftReport};
use crate::error::{CodeintelError, Result};
use crate::federation::{self, CrossRepoDep, FederatedCrate, FederationReport};
use crate::ids::PREFIX_ALL_REPOS;
use crate::impact::{self, ImpactReport, Neighbor};
use crate::indexer::{decode_obj, open_store};
use crate::model::RepoRecord;
use crate::query::{Cycle, LayerViolation, PatternHit, QueryEngine};
use bindb::BinderStore;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// Semantic architecture surface for autonomous agents.
///
/// Open a store path. Do not query WAL, object ids, or namespaces directly.
pub struct Architecture {
    store: BinderStore,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub repositories: Vec<RepoSummary>,
    pub crates: usize,
    pub modules: usize,
    pub files: usize,
    pub symbols: usize,
    pub doctrine: Vec<String>,
    pub dependency_overview: Vec<CrossRepoDep>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoSummary {
    pub name: String,
    pub sha: Option<String>,
    pub crates: usize,
    pub modules: usize,
    pub files: usize,
    pub symbols: usize,
    pub doctrine: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctrineView {
    pub repositories: Vec<RepoDoctrine>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoDoctrine {
    pub repository: String,
    pub report: ConformanceReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpactView {
    pub repositories: Vec<String>,
    pub report: ImpactReport,
    pub cross_repo_dependents: Vec<FederatedCrate>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyView {
    pub target: String,
    pub incoming: Vec<Neighbor>,
    pub outgoing: Vec<Neighbor>,
    pub chains: Vec<Vec<String>>,
    pub circular: Vec<Cycle>,
    pub layering: Vec<LayerViolation>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleView {
    pub scope: String,
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayeringView {
    pub repository: String,
    pub from_crate: String,
    pub to_crate: String,
    pub from_rank: u32,
    pub to_rank: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FederationView {
    pub dependency_graph: Vec<CrossRepoDep>,
    pub ownership_graph: Vec<FederatedCrate>,
    pub shared_libraries: Vec<FederatedCrate>,
    pub architectural_boundaries: Vec<CrossRepoDep>,
    pub impact_across_repositories: Vec<CrossRepoDep>,
    pub report: FederationReport,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanReport {
    pub request: String,
    pub interpretation: String,
    /// `file`, `crate`, or `none` — file matches win so the inspection set stays small.
    pub matched_kind: String,
    pub repositories: Vec<String>,
    pub rules_must_remain_satisfied: Vec<String>,
    pub likely_affected: Vec<String>,
    pub matching_patterns: Vec<PatternHit>,
    pub inspect_before_change: Vec<String>,
    pub smallest_file_set: Vec<String>,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
}

impl Architecture {
    /// Open an existing bindb store. This is the only storage detail agents need.
    pub fn open(store_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            store: open_store(store_path.as_ref())?,
        })
    }

    pub fn close(self) -> Result<()> {
        self.store.close()?;
        Ok(())
    }

    pub fn summary(&self) -> Result<Summary> {
        let names = repo_names(&self.store)?;
        let mut repositories = Vec::new();
        let mut crates = 0usize;
        let mut modules = 0usize;
        let mut files = 0usize;
        let mut symbols = 0usize;
        let mut doctrine = BTreeSet::new();
        for name in &names {
            let q = QueryEngine::open(&self.store, name);
            let s = q.summary()?;
            crates += s.crates;
            modules += s.modules;
            files += s.files;
            symbols += s.symbols;
            doctrine.insert(s.doctrine.clone());
            repositories.push(RepoSummary {
                name: name.clone(),
                sha: s.sha,
                crates: s.crates,
                modules: s.modules,
                files: s.files,
                symbols: s.symbols,
                doctrine: s.doctrine,
            });
        }
        let fed = federation::analyze(&self.store)?;
        Ok(Summary {
            repositories,
            crates,
            modules,
            files,
            symbols,
            doctrine: doctrine.into_iter().collect(),
            dependency_overview: fed.cross_repo_deps,
            primary_source: "bindb",
            source_files_opened: 0,
        })
    }

    pub fn doctrine(&self) -> Result<DoctrineView> {
        let mut repositories = Vec::new();
        for name in repo_names(&self.store)? {
            let q = QueryEngine::open(&self.store, &name);
            let crates: Vec<String> = q.crates()?.into_iter().map(|c| c.name).collect();
            let report = Doctrine::detect(&crates).evaluate(&q)?;
            repositories.push(RepoDoctrine {
                repository: name,
                report,
            });
        }
        Ok(DoctrineView {
            repositories,
            primary_source: "bindb",
            source_files_opened: 0,
        })
    }

    pub fn impact(&self, target: &str) -> Result<ImpactView> {
        let (repo, spec) = resolve_repo_target(&self.store, target)?;
        let q = QueryEngine::open(&self.store, &repo);
        let report = impact::analyze(&q, &spec)?;
        let mut repositories = BTreeSet::new();
        repositories.insert(repo.clone());
        let cross_repo_dependents = match federation::impact(&self.store, target) {
            Ok(fed) => {
                repositories.insert(fed.target_repo.clone());
                for d in &fed.dependents {
                    repositories.insert(d.repo.clone());
                }
                fed.dependents
            }
            Err(_) => Vec::new(),
        };
        Ok(ImpactView {
            repositories: repositories.into_iter().collect(),
            report,
            cross_repo_dependents,
            primary_source: "bindb",
            source_files_opened: 0,
        })
    }

    pub fn dependencies(&self, target: &str) -> Result<DependencyView> {
        let (repo, spec) = resolve_repo_target(&self.store, target)?;
        let q = QueryEngine::open(&self.store, &repo);
        let report = impact::analyze(&q, &spec)?;
        let graph = q.crate_dep_graph()?;
        let crate_name = crate_label(&report);
        let chains = if let Some(c) = &crate_name {
            outgoing_chains(&graph, c, 6)
        } else {
            Vec::new()
        };
        let circular = q
            .circular_dependencies()?
            .into_iter()
            .filter(|c| crate_name.as_ref().is_some_and(|n| c.nodes.contains(n)))
            .collect();
        let layering = q
            .layering_violations()?
            .into_iter()
            .filter(|v| {
                crate_name
                    .as_ref()
                    .is_some_and(|n| v.from_crate == *n || v.to_crate == *n)
            })
            .collect();
        Ok(DependencyView {
            target: spec,
            incoming: report.incoming,
            outgoing: report.outgoing,
            chains,
            circular,
            layering,
            primary_source: "bindb",
            source_files_opened: 0,
        })
    }

    pub fn cycles(&self) -> Result<Vec<CycleView>> {
        let mut out = Vec::new();
        for name in repo_names(&self.store)? {
            let q = QueryEngine::open(&self.store, &name);
            for c in q.circular_dependencies()? {
                out.push(CycleView {
                    scope: name.clone(),
                    nodes: c.nodes,
                });
            }
        }
        let fed = federation::analyze(&self.store)?;
        for nodes in fed.cycles {
            out.push(CycleView {
                scope: "federation".into(),
                nodes,
            });
        }
        Ok(out)
    }

    pub fn layering(&self) -> Result<Vec<LayeringView>> {
        let mut out = Vec::new();
        for name in repo_names(&self.store)? {
            let q = QueryEngine::open(&self.store, &name);
            for v in q.layering_violations()? {
                out.push(LayeringView {
                    repository: name.clone(),
                    from_crate: v.from_crate,
                    to_crate: v.to_crate,
                    from_rank: v.from_rank,
                    to_rank: v.to_rank,
                });
            }
        }
        Ok(out)
    }

    pub fn drift(&self, from_sha: &str, to_sha: &str) -> Result<DriftReport> {
        analyze_indexed(&self.store, from_sha, to_sha)
    }

    pub fn federation(&self) -> Result<FederationView> {
        let report = federation::analyze(&self.store)?;
        let mut shared: HashMap<String, FederatedCrate> = HashMap::new();
        for e in &report.cross_repo_deps {
            shared.insert(
                format!("{}/{}", e.to_repo, e.to_crate),
                FederatedCrate {
                    repo: e.to_repo.clone(),
                    name: e.to_crate.clone(),
                },
            );
        }
        let mut shared_libraries: Vec<FederatedCrate> = shared.into_values().collect();
        shared_libraries.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.name.cmp(&b.name)));
        Ok(FederationView {
            dependency_graph: report.cross_repo_deps.clone(),
            ownership_graph: report.crates.clone(),
            shared_libraries,
            architectural_boundaries: report.cross_repo_deps.clone(),
            impact_across_repositories: report.cross_repo_deps.clone(),
            report,
            primary_source: "bindb",
            source_files_opened: 0,
        })
    }

    /// Graph-first implementation plan. Source paths are payload strings, not file reads.
    ///
    /// Matching prefers file paths named in the request, then crate names. That keeps
    /// the inspection set to implicated files instead of every payload in a crate.
    pub fn plan(&self, request: &str) -> Result<PlanReport> {
        let names = repo_names(&self.store)?;
        let needle = request.to_lowercase();
        let mut matched_files: Vec<(String, String)> = Vec::new();
        let mut matched_crates: Vec<(String, String)> = Vec::new();
        let mut likely_affected = BTreeSet::new();
        let mut matching_patterns = Vec::new();
        let mut rules = BTreeSet::new();
        let mut repos = BTreeSet::new();

        for name in &names {
            let q = QueryEngine::open(&self.store, name);
            let crates = q.crates()?;
            let crate_names: Vec<String> = crates.iter().map(|c| c.name.clone()).collect();
            for f in q.files()? {
                if request_matches_file(&needle, &f.path) {
                    matched_files.push((name.clone(), f.path));
                    repos.insert(name.clone());
                }
            }
            for c in &crates {
                if needle.contains(&c.name.to_lowercase()) {
                    matched_crates.push((name.clone(), c.name.clone()));
                    likely_affected.insert(format!("{}/{}", name, c.name));
                    repos.insert(name.clone());
                }
            }
            let doctrine = Doctrine::detect(&crate_names);
            for rule in &doctrine.rules {
                let id = match rule {
                    crate::doctrine::Rule::Layering { id, .. } => id,
                    crate::doctrine::Rule::ForbiddenDep { id, .. } => id,
                    crate::doctrine::Rule::Ownership { id, .. } => id,
                    crate::doctrine::Rule::Acyclic { id } => id,
                };
                rules.insert(format!("{name}:{id}"));
            }
        }

        let prefer_files = !matched_files.is_empty();
        let matched_kind = if prefer_files {
            "file"
        } else if !matched_crates.is_empty() {
            "crate"
        } else {
            "none"
        };

        if !prefer_files && !matched_crates.is_empty() {
            let matched_repos: BTreeSet<String> =
                matched_crates.iter().map(|(r, _)| r.clone()).collect();
            for name in matched_repos {
                let q = QueryEngine::open(&self.store, &name);
                matching_patterns.extend(q.patterns()?);
                for g in q.duplicates()? {
                    matching_patterns.push(PatternHit {
                        kind: "structural_duplicate".into(),
                        detail: g.members.join(", "),
                    });
                }
            }
        }

        let mut inspect = BTreeSet::new();
        let mut files = BTreeSet::new();
        let interpretation = if prefer_files {
            format!(
                "matched files {}",
                matched_files
                    .iter()
                    .map(|(r, p)| format!("{r}:{p}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if matched_crates.is_empty() {
            "no crate name or file path in the request matched the graph; inspect nothing until the agent names a crate, file, or symbol present in bindb".into()
        } else {
            format!(
                "matched {}",
                matched_crates
                    .iter()
                    .map(|(r, c)| format!("{r}/{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let impact_targets: Vec<(String, String)> = if prefer_files {
            matched_files
        } else {
            matched_crates
        };

        for (repo, spec) in &impact_targets {
            let q = QueryEngine::open(&self.store, repo);
            let impact = impact::analyze(&q, spec)?;
            for f in &impact.inspection_set.files {
                inspect.insert(f.clone());
                files.insert(f.clone());
            }
            for c in &impact.inspection_set.crates {
                likely_affected.insert(format!("{repo}/{c}"));
            }
            for d in &impact.transitive_crate_dependents {
                likely_affected.insert(format!("{repo}/{d}"));
            }
            if let Some(crate_name) = crate_label(&impact) {
                if let Ok(fed) = federation::impact(&self.store, &crate_name) {
                    for d in fed.dependents {
                        repos.insert(d.repo.clone());
                        likely_affected.insert(format!("{}/{}", d.repo, d.name));
                    }
                }
            }
        }

        let smallest_file_set: Vec<String> = files.into_iter().collect();
        Ok(PlanReport {
            request: request.to_string(),
            interpretation,
            matched_kind: matched_kind.into(),
            repositories: repos.into_iter().collect(),
            rules_must_remain_satisfied: rules.into_iter().collect(),
            likely_affected: likely_affected.into_iter().collect(),
            matching_patterns,
            inspect_before_change: inspect.into_iter().collect(),
            smallest_file_set,
            primary_source: "bindb",
            source_files_opened: 0,
        })
    }
}

/// True when the request names this indexed path (multi-component suffix or distinctive filename).
fn request_matches_file(needle: &str, path: &str) -> bool {
    let path_lc = path.to_lowercase().replace('\\', "/");
    if needle.contains(&path_lc) {
        return true;
    }
    let parts: Vec<&str> = path_lc.split('/').filter(|s| !s.is_empty()).collect();
    for i in 0..parts.len() {
        let suffix = parts[i..].join("/");
        if suffix.contains('/') && needle.contains(&suffix) {
            return true;
        }
    }
    if let Some(fname) = parts.last() {
        let stem = fname.rsplit_once('.').map(|(s, _)| s).unwrap_or(fname);
        if stem.len() >= 8 && needle.contains(fname) {
            return true;
        }
    }
    false
}

fn repo_names(store: &BinderStore) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for v in store.list(PREFIX_ALL_REPOS)? {
        let rec: RepoRecord = decode_obj(&v)?;
        names.push(rec.name);
    }
    names.sort();
    Ok(names)
}

fn resolve_repo_target(store: &BinderStore, target: &str) -> Result<(String, String)> {
    let target = target.trim();
    if let Some((repo, spec)) = target.split_once('/') {
        if repo_names(store)?.iter().any(|n| n == repo) {
            return Ok((repo.to_string(), spec.to_string()));
        }
    }
    for name in repo_names(store)? {
        let q = QueryEngine::open(store, &name);
        if q.crates()?.iter().any(|c| c.name == target) {
            return Ok((name, target.to_string()));
        }
        if q.files()?.iter().any(|f| f.path == target) {
            return Ok((name, target.to_string()));
        }
        if q.modules()?.iter().any(|m| m.path == target) {
            return Ok((name, target.to_string()));
        }
        if q.symbols()?
            .iter()
            .any(|s| s.qualname == target || s.name == target)
        {
            return Ok((name, target.to_string()));
        }
    }
    Err(CodeintelError::Other(format!(
        "target {target} is not in the indexed graph"
    )))
}

fn crate_label(report: &ImpactReport) -> Option<String> {
    match report.target_kind {
        crate::impact::TargetKind::Crate => Some(report.target.clone()),
        _ => report.inspection_set.crates.first().cloned(),
    }
}

fn outgoing_chains(edges: &[(String, String)], from: &str, max_depth: usize) -> Vec<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in edges {
        adj.entry(a.as_str()).or_default().push(b.as_str());
    }
    let mut out = Vec::new();
    let mut stack = vec![vec![from]];
    while let Some(path) = stack.pop() {
        if path.len() > max_depth {
            continue;
        }
        let last = *path.last().unwrap();
        let Some(nexts) = adj.get(last) else {
            if path.len() > 1 {
                out.push(path.iter().map(|s| (*s).to_string()).collect());
            }
            continue;
        };
        if nexts.is_empty() && path.len() > 1 {
            out.push(path.iter().map(|s| (*s).to_string()).collect());
            continue;
        }
        let mut branched = false;
        for n in nexts {
            if path.contains(n) {
                continue;
            }
            branched = true;
            let mut p = path.clone();
            p.push(n);
            stack.push(p);
        }
        if !branched && path.len() > 1 {
            out.push(path.iter().map(|s| (*s).to_string()).collect());
        }
    }
    out.truncate(20);
    out
}
