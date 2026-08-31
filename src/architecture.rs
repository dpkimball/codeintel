//! Agent-facing architecture API. Operations are semantic; bindb stays behind this type.

use crate::doctrine::{ConformanceReport, Doctrine};
use crate::drift::{analyze_indexed, DriftReport};
use crate::error::{CodeintelError, Result};
use crate::federation::{self, CrossRepoDep, FederatedCrate, FederationReport};
use crate::ids::PREFIX_ALL_REPOS;
use crate::impact::{self, ImpactReport, Neighbor};
use crate::indexer::{decode_obj, open_store};
use crate::model::{FileRecord, RepoRecord, SymbolRecord};
use crate::query::{Cycle, LayerViolation, PatternHit, QueryEngine};
use bindb::{BinderStore, ObjectId};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
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
pub struct IssueRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub repo: String,
    /// First 1 000 chars of the issue body. Empty when body is blank.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub body_excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub repo: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocRef {
    pub path: String,
    pub title: String,
    pub repo: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchReason {
    pub kind: String,
    pub label: String,
    pub repo: String,
}

/// Role of a file in the inspection set relative to the matched symbol.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    Definition,
    CallSite,
    Implementation,
    Test,
    EntryPoint,
    Dependency,
}

impl RoleKind {
    fn as_str(&self) -> &'static str {
        match self {
            RoleKind::Definition => "definition",
            RoleKind::CallSite => "call_site",
            RoleKind::Implementation => "implementation",
            RoleKind::Test => "test",
            RoleKind::EntryPoint => "entry_point",
            RoleKind::Dependency => "dependency",
        }
    }

    fn priority(&self) -> u8 {
        match self {
            RoleKind::Definition => 0,
            RoleKind::Implementation => 1,
            RoleKind::CallSite => 2,
            RoleKind::Test => 3,
            RoleKind::EntryPoint => 4,
            RoleKind::Dependency => 5,
        }
    }

    fn action(&self) -> &'static str {
        match self {
            RoleKind::Definition => "extend or add here",
            RoleKind::Implementation => "add impl",
            RoleKind::CallSite => "handle new case",
            RoleKind::Test => "add test coverage",
            RoleKind::EntryPoint => "wire up",
            RoleKind::Dependency => "context only",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRole {
    pub path: String,
    pub role: String,
    pub note: String,
}

/// An ordered change site for structural task patterns.
#[derive(Debug, Clone, Serialize)]
pub struct TouchPoint {
    pub file: String,
    pub role: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftedFile {
    pub path: String,
    pub repo: String,
    /// `"added"` or `"removed"` relative to the from-SHA snapshot.
    pub change: String,
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
    /// GitHub issues whose content overlaps with the query (top 10 by token overlap).
    pub linked_issues: Vec<IssueRef>,
    /// GitHub pull requests whose content overlaps with the query (top 8 by token overlap).
    pub linked_prs: Vec<PrRef>,
    /// Markdown docs whose content overlaps with the query (top 8 by token overlap).
    pub linked_docs: Vec<DocRef>,
    /// Structured explanation of what matched and why.
    pub reasoning: Vec<MatchReason>,
    /// Issue numbers linked to the inspection set via REL_RESOLVES edges from git log.
    pub origin_issues: Vec<u64>,
    /// Files added or removed in the matched repo(s) since `since_sha` that overlap
    /// with the inspection set. Absent when `since_sha` is not provided.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_changes: Vec<DriftedFile>,
    /// Per-file role classification relative to the matched symbol.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_roles: Vec<FileRole>,
    /// Ordered change sites for structural task patterns (symbol/module matches only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub touch_sequence: Vec<TouchPoint>,
    /// One-sentence summary of the primary definition and top call sites.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub matched_summary: String,
    /// Present when touch_sequence is non-empty; notes that action text assumes a structural change.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub task_hint: String,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
    /// Pre-authored Q&A pairs from the question bank whose tokens overlap with the query.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub guidance: Vec<GuidanceItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuidanceItem {
    pub id: String,
    pub question: String,
    pub answer: String,
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

    /// Graph-first implementation plan with drift context.
    ///
    /// `since_sha` must already be indexed (via `codeintel index`). `recent_changes` in the
    /// returned report lists files in the matched repos that were added or removed between
    /// `since_sha` and the current HEAD and also appear in the inspection set.
    pub fn plan_since(&self, request: &str, since_sha: &str) -> Result<PlanReport> {
        self.plan_impl(request, Some(since_sha))
    }

    /// Graph-first implementation plan. Source paths are payload strings, not file reads.
    ///
    /// Matching order: file paths named in the request, then config keys/tokens
    /// and docker-shipping intent, then crate names as whole tokens. Inspect paths are `repo:path`.
    pub fn plan(&self, request: &str) -> Result<PlanReport> {
        self.plan_impl(request, None)
    }

    fn plan_impl(&self, request: &str, since_sha: Option<&str>) -> Result<PlanReport> {
        let names = repo_names(&self.store)?;
        let needle = request.to_lowercase();
        let tokens = request_tokens(&needle);
        let docker_intent = docker_shipping_intent(&needle);
        let mut matched_files: Vec<(String, String)> = Vec::new();
        let mut matched_configs: Vec<(String, String)> = Vec::new();
        let mut matched_crates: Vec<(String, String)> = Vec::new();
        let mut matched_symbols: Vec<(String, String)> = Vec::new();
        let mut likely_affected = BTreeSet::new();
        let mut matching_patterns = Vec::new();
        let mut rules = BTreeSet::new();
        let mut repos = BTreeSet::new();

        // Repo-anchor scoping: when a query token exactly matches a known repo name,
        // restrict the search to only those repos. This prevents "codeintel index_prs"
        // from matching binder-ai's knowledge/codeintel.rs module and polluting results.
        let anchor_repos: BTreeSet<&str> = names
            .iter()
            .filter(|n| tokens.contains(n.as_str()))
            .map(|n| n.as_str())
            .collect();

        for name in &names {
            if !anchor_repos.is_empty() && !anchor_repos.contains(name.as_str()) {
                continue;
            }
            let q = QueryEngine::open(&self.store, name);
            let crates = q.crates()?;
            let crate_names: Vec<String> = crates.iter().map(|c| c.name.clone()).collect();
            for f in q.files()? {
                if request_matches_file(&needle, &f.path) {
                    matched_files.push((name.clone(), f.path));
                    repos.insert(name.clone());
                }
            }
            for ck in q.config_keys()? {
                if request_matches_config(&needle, &ck, docker_intent) {
                    matched_configs.push((name.clone(), ck.file_path.clone()));
                    repos.insert(name.clone());
                }
            }
            for c in &crates {
                if tokens.contains(&c.name.to_lowercase()) {
                    matched_crates.push((name.clone(), c.name.clone()));
                    likely_affected.insert(format!("{}/{}", name, c.name));
                    repos.insert(name.clone());
                }
            }
            // Module-path segment matching: if a token (len >= 5) matches a
            // module path segment, treat the owning crate as a match. This
            // lets queries like "hygiene workflow" or "routing selector" resolve
            // without requiring the caller to know the crate name.
            if matched_files.is_empty() && matched_configs.is_empty() {
                'module_outer: for m in q.modules()? {
                    let segments: Vec<&str> = m.path.split("::").collect();
                    for seg in &segments {
                        let seg_lc = seg.to_lowercase();
                        if seg_lc.len() >= 5 && tokens.contains(&seg_lc) {
                            if !matched_crates
                                .iter()
                                .any(|(r, c)| r == name && *c == m.crate_name)
                            {
                                matched_crates.push((name.clone(), m.crate_name.clone()));
                                likely_affected.insert(format!("{}/{}", name, m.crate_name));
                                repos.insert(name.clone());
                            }
                            break 'module_outer;
                        }
                    }
                }
            }
            // Symbol name matching. Always runs (symbols beat crates in priority; see
            // matched_kind and impact_targets selection below).
            if matched_files.is_empty() && matched_configs.is_empty() {
                for s in q.symbols()? {
                    let name_lc = s.name.to_lowercase();
                    for tok in &tokens {
                        if tok.len() >= 4
                            && *tok == name_lc
                            && !matched_symbols
                                .iter()
                                .any(|(r, q)| r == name && q == &s.qualname)
                        {
                            matched_symbols.push((name.clone(), s.qualname.clone()));
                            repos.insert(name.clone());
                            break;
                        }
                    }
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

        matched_configs.sort();
        matched_configs.dedup();

        let prefer_files = !matched_files.is_empty();
        let prefer_config = !prefer_files && !matched_configs.is_empty();
        // matched_kind reflects the actual impact granularity used, not just what was found.
        // Symbols beat crates: when a symbol matches, use symbol-level impact even if the
        // enclosing crate also matched by name (crate names are too broad to win).
        let matched_kind = if prefer_files {
            "file"
        } else if prefer_config {
            "config"
        } else if !matched_symbols.is_empty() {
            "symbol"
        } else if !matched_crates.is_empty() {
            "crate"
        } else {
            "none"
        };

        if !prefer_files
            && !prefer_config
            && (!matched_crates.is_empty() || !matched_symbols.is_empty())
        {
            let matched_repos: BTreeSet<String> = matched_crates
                .iter()
                .chain(matched_symbols.iter())
                .map(|(r, _)| r.clone())
                .collect();
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

        let matched_files_for_reasoning = matched_files.clone();
        let mut inspect = BTreeSet::new();
        let mut files = BTreeSet::new();
        let interpretation = if prefer_files {
            format!(
                "matched files {}",
                matched_files
                    .iter()
                    .map(|(r, p)| qualify_path(r, p))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if prefer_config {
            format!(
                "matched config {}",
                matched_configs
                    .iter()
                    .map(|(r, p)| qualify_path(r, p))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if matched_crates.is_empty() && matched_symbols.is_empty() {
            "no crate name, file path, config key, or symbol in the request matched the graph; inspect nothing until the agent names a crate, file, config key, or symbol present in bindb".into()
        } else if matched_crates.is_empty() {
            format!(
                "matched symbols {}",
                matched_symbols
                    .iter()
                    .map(|(r, q)| qualify_path(r, q))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
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

        if prefer_config {
            let mut config_files = matched_configs.clone();
            for (repo, path) in &matched_configs {
                let Some(dir) = crate::config_keys::helm_chart_dir(path) else {
                    continue;
                };
                let q = QueryEngine::open(&self.store, repo);
                for f in q.files()? {
                    if crate::config_keys::helm_chart_dir(&f.path).as_deref() == Some(dir.as_str())
                        && crate::config_keys::is_helm_values_file(&f.path)
                    {
                        config_files.push((repo.clone(), f.path));
                    }
                }
            }
            config_files.sort();
            config_files.dedup();
            for (repo, path) in &config_files {
                let qpath = qualify_path(repo, path);
                inspect.insert(qpath.clone());
                files.insert(qpath);
                let q = QueryEngine::open(&self.store, repo);
                if let Some(f) = q.files()?.into_iter().find(|f| &f.path == path) {
                    if let Some(c) = f.crate_name {
                        likely_affected.insert(format!("{repo}/{c}"));
                    }
                }
                if docker_intent && path.rsplit('/').next() == Some("dpk.toml") {
                    let df = dockerfile_beside(path);
                    if q.files()?.iter().any(|f| f.path == df) {
                        let qpath = qualify_path(repo, &df);
                        inspect.insert(qpath.clone());
                        files.insert(qpath);
                    }
                }
            }
        } else {
            let impact_targets: Vec<(String, String)> = if prefer_files {
                matched_files
            } else if !matched_symbols.is_empty() {
                matched_symbols.clone()
            } else {
                matched_crates.clone()
            };

            for (repo, spec) in &impact_targets {
                let q = QueryEngine::open(&self.store, repo);
                let impact = impact::analyze(&q, spec)?;
                for f in &impact.inspection_set.files {
                    let qpath = qualify_path(repo, f);
                    inspect.insert(qpath.clone());
                    files.insert(qpath);
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
        }

        // Always include orientation docs (CLAUDE.md, README.md) from matched repos so
        // agents get architectural context even when symbol-level impact is very narrow.
        if !repos.is_empty() {
            for name in &repos {
                let q = QueryEngine::open(&self.store, name);
                for f in q.files()? {
                    if matches!(f.path.as_str(), "CLAUDE.md" | "README.md") {
                        let qpath = qualify_path(name, &f.path);
                        inspect.insert(qpath.clone());
                        files.insert(qpath);
                    }
                }
            }
        }

        let smallest_file_set: Vec<String> = files.into_iter().collect();

        // semantic matching: issues and docs with ≥ 2 query-token overlap
        let match_tokens: BTreeSet<String> = tokens
            .iter()
            .filter(|t| t.len() >= 4 || crate::ids::SHORT_TECHNICAL_TERMS.contains(&t.as_str()))
            .cloned()
            .collect();
        let linked_issues = semantic_issues(&self.store, &names, &match_tokens)?;
        let linked_prs = semantic_prs(&self.store, &names, &match_tokens)?;
        let linked_docs = semantic_docs(&self.store, &names, &match_tokens)?;
        let guidance = semantic_guidance(&self.store, &names, &match_tokens, &anchor_repos)?;

        let mut reasoning: Vec<MatchReason> = Vec::new();
        for (repo, path) in &matched_files_for_reasoning {
            reasoning.push(MatchReason {
                kind: "file".into(),
                label: path.clone(),
                repo: repo.clone(),
            });
        }
        // Symbols drive the impact set only when no crate also matched; otherwise
        // crate-level impact is used, so show crate reasons (not symbol reasons).
        if matched_crates.is_empty() {
            for (repo, qualname) in &matched_symbols {
                reasoning.push(MatchReason {
                    kind: "symbol".into(),
                    label: qualname.clone(),
                    repo: repo.clone(),
                });
            }
        }
        for (repo, cname) in &matched_crates {
            reasoning.push(MatchReason {
                kind: "crate".into(),
                label: cname.clone(),
                repo: repo.clone(),
            });
        }
        for issue in &linked_issues {
            let title_short: String = issue.title.chars().take(60).collect();
            reasoning.push(MatchReason {
                kind: "issue".into(),
                label: format!("#{} {}", issue.number, title_short),
                repo: issue.repo.clone(),
            });
        }
        for pr in &linked_prs {
            let title_short: String = pr.title.chars().take(60).collect();
            reasoning.push(MatchReason {
                kind: "pr".into(),
                label: format!("#{} {}", pr.number, title_short),
                repo: pr.repo.clone(),
            });
        }
        for doc in &linked_docs {
            reasoning.push(MatchReason {
                kind: "doc".into(),
                label: doc.title.clone(),
                repo: doc.repo.clone(),
            });
        }

        // origin_issues: issue numbers linked to the inspection set via git log REL_RESOLVES edges.
        let mut origin_issues_set: BTreeSet<u64> = BTreeSet::new();
        for repo_name in &repos {
            let repo_prefix = format!("{repo_name}:");
            let file_paths_for_repo: Vec<&str> = smallest_file_set
                .iter()
                .filter_map(|full| full.strip_prefix(repo_prefix.as_str()))
                .collect();
            if !file_paths_for_repo.is_empty() {
                if let Ok(nums) = crate::commit_issues::origin_issues_for_files(
                    &self.store,
                    repo_name,
                    &file_paths_for_repo,
                ) {
                    origin_issues_set.extend(nums);
                }
            }
        }
        let origin_issues: Vec<u64> = origin_issues_set.into_iter().collect();

        // recent_changes: files added/removed in matched repos between since_sha and HEAD,
        // filtered to those that overlap with the inspection set.
        let recent_changes = if let Some(since) = since_sha {
            compute_recent_changes(&self.store, &repos, &smallest_file_set, since)
        } else {
            Vec::new()
        };

        let (file_roles, touch_sequence, matched_summary, task_hint) = enrich_file_roles(
            &self.store,
            &smallest_file_set,
            &matched_symbols,
            matched_kind,
        );

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
            linked_issues,
            linked_prs,
            linked_docs,
            reasoning,
            origin_issues,
            recent_changes,
            file_roles,
            touch_sequence,
            matched_summary,
            task_hint,
            primary_source: "bindb",
            source_files_opened: 0,
            guidance,
        })
    }
}

/// Classify every file in the inspection set by its role relative to matched symbols.
/// Returns (file_roles, touch_sequence, matched_summary, task_hint).
fn enrich_file_roles(
    store: &BinderStore,
    smallest_file_set: &[String],
    matched_symbols: &[(String, String)],
    matched_kind: &str,
) -> (Vec<FileRole>, Vec<TouchPoint>, String, String) {
    if matched_symbols.is_empty() || smallest_file_set.is_empty() {
        return (Vec::new(), Vec::new(), String::new(), String::new());
    }

    // Build per-qualified-path sets: definition, call_site (non-test), call_site (test),
    // implementation (non-test), implementation (test).
    let mut definition_paths: HashSet<String> = HashSet::new();
    let mut call_site_paths: HashSet<String> = HashSet::new();
    let mut call_site_test_paths: HashSet<String> = HashSet::new();
    let mut impl_paths: HashSet<String> = HashSet::new();
    let mut impl_test_paths: HashSet<String> = HashSet::new();

    for (repo, qualname) in matched_symbols {
        let q = QueryEngine::open(store, repo);
        let sym_id = crate::ids::symbol_id(repo, qualname);

        // Definition: file whose symbol_ids contains this symbol's object ID.
        for full_path in smallest_file_set {
            let Some((file_repo, file_path)) = full_path.split_once(':') else {
                continue;
            };
            if file_repo != repo {
                continue;
            }
            let fid = crate::ids::file_id(repo, file_path);
            if let Ok(Some(raw)) = store.get(&fid) {
                if let Ok(rec) = decode_obj::<FileRecord>(&raw) {
                    if rec.symbol_ids.contains(&sym_id.0) {
                        definition_paths.insert(full_path.clone());
                    }
                }
            }
        }

        // Call sites: symbols with REL_CALLS → matched symbol; resolve to file_path.
        if let Ok(caller_ids) = q.callers(qualname) {
            for caller_id in caller_ids {
                if let Ok(Some(raw)) = store.get(&ObjectId(caller_id)) {
                    if let Ok(sym_rec) = decode_obj::<SymbolRecord>(&raw) {
                        let full = qualify_path(repo, &sym_rec.file_path);
                        if sym_rec.in_test {
                            call_site_test_paths.insert(full);
                        } else {
                            call_site_paths.insert(full);
                        }
                    }
                }
            }
        }

        // Implementations: symbols with REL_IMPLEMENTS → matched symbol.
        if let Ok(impl_ids) = q.trait_impls(qualname) {
            for impl_id in impl_ids {
                if let Ok(Some(raw)) = store.get(&ObjectId(impl_id)) {
                    if let Ok(sym_rec) = decode_obj::<SymbolRecord>(&raw) {
                        let full = qualify_path(repo, &sym_rec.file_path);
                        if sym_rec.in_test {
                            impl_test_paths.insert(full);
                        } else {
                            impl_paths.insert(full);
                        }
                    }
                }
            }
        }
    }

    // Classify each file in priority order.
    let mut file_roles: Vec<FileRole> = smallest_file_set
        .iter()
        .map(|full_path| {
            let role = if definition_paths.contains(full_path) {
                RoleKind::Definition
            } else if call_site_test_paths.contains(full_path)
                || impl_test_paths.contains(full_path)
            {
                RoleKind::Test
            } else if call_site_paths.contains(full_path) {
                RoleKind::CallSite
            } else if impl_paths.contains(full_path) {
                RoleKind::Implementation
            } else if is_entry_point(full_path) {
                RoleKind::EntryPoint
            } else {
                RoleKind::Dependency
            };
            let sym_name = matched_symbols
                .first()
                .map(|(_, q)| q.rsplit("::").next().unwrap_or(q.as_str()))
                .unwrap_or("");
            let note = match role {
                RoleKind::Definition => format!("defines {sym_name}"),
                RoleKind::CallSite => format!("calls {sym_name}"),
                RoleKind::Implementation => format!("implements {sym_name}"),
                RoleKind::Test => format!("tests {sym_name}"),
                RoleKind::EntryPoint => "binary entry point".into(),
                RoleKind::Dependency => "transitive dependency".into(),
            };
            FileRole {
                path: full_path.clone(),
                role: role.as_str().into(),
                note,
            }
        })
        .collect();

    // Build touch_sequence for symbol/module matches only (skip Dependency entries).
    let touch_sequence = if matched_kind == "symbol" || matched_kind == "module" {
        let mut ordered: Vec<(u8, &FileRole)> = file_roles
            .iter()
            .filter(|fr| fr.role != "dependency")
            .filter_map(|fr| {
                let prio = match fr.role.as_str() {
                    "definition" => Some(RoleKind::Definition.priority()),
                    "implementation" => Some(RoleKind::Implementation.priority()),
                    "call_site" => Some(RoleKind::CallSite.priority()),
                    "test" => Some(RoleKind::Test.priority()),
                    "entry_point" => Some(RoleKind::EntryPoint.priority()),
                    _ => None,
                }?;
                Some((prio, fr))
            })
            .collect();
        ordered.sort_by_key(|(p, _)| *p);
        ordered
            .into_iter()
            .map(|(_, fr)| {
                let action = match fr.role.as_str() {
                    "definition" => RoleKind::Definition.action(),
                    "implementation" => RoleKind::Implementation.action(),
                    "call_site" => RoleKind::CallSite.action(),
                    "test" => RoleKind::Test.action(),
                    "entry_point" => RoleKind::EntryPoint.action(),
                    _ => "review",
                };
                TouchPoint {
                    file: fr.path.clone(),
                    role: fr.role.clone(),
                    action: action.into(),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // One-sentence summary: definition + top call site + top test.
    let def_path = file_roles
        .iter()
        .find(|fr| fr.role == "definition")
        .map(|fr| fr.path.split_once(':').map(|(_, p)| p).unwrap_or(&fr.path));
    let call_path = file_roles
        .iter()
        .find(|fr| fr.role == "call_site")
        .map(|fr| fr.path.split_once(':').map(|(_, p)| p).unwrap_or(&fr.path));
    let test_path = file_roles
        .iter()
        .find(|fr| fr.role == "test")
        .map(|fr| fr.path.split_once(':').map(|(_, p)| p).unwrap_or(&fr.path));
    let sym_name = matched_symbols
        .first()
        .map(|(_, q)| q.rsplit("::").next().unwrap_or(q.as_str()))
        .unwrap_or("");
    let matched_summary = match (def_path, call_path, test_path) {
        (Some(d), Some(c), Some(t)) => {
            format!("{sym_name} defined in {d}; called from {c}; tested in {t}")
        }
        (Some(d), Some(c), None) => format!("{sym_name} defined in {d}; called from {c}"),
        (Some(d), None, Some(t)) => format!("{sym_name} defined in {d}; tested in {t}"),
        (Some(d), None, None) => format!("{sym_name} defined in {d}"),
        (None, Some(c), _) => format!("{sym_name} called from {c}"),
        _ => {
            if sym_name.is_empty() {
                String::new()
            } else {
                format!("matched {sym_name}")
            }
        }
    };

    let task_hint = if !touch_sequence.is_empty() {
        "touch_sequence assumes a structural change (add/extend); for read-only investigation, ignore action text".into()
    } else {
        String::new()
    };

    // Sort file_roles: definitions first, then by priority, keeping stability.
    file_roles.sort_by_key(|fr| match fr.role.as_str() {
        "definition" => 0u8,
        "implementation" => 1,
        "call_site" => 2,
        "test" => 3,
        "entry_point" => 4,
        _ => 5,
    });

    (file_roles, touch_sequence, matched_summary, task_hint)
}

fn is_entry_point(full_path: &str) -> bool {
    let path = full_path
        .split_once(':')
        .map(|(_, p)| p)
        .unwrap_or(full_path);
    path.contains("/src/bin/") || path.ends_with("/main.rs") || path == "main.rs"
}

/// `repo:path` so colliding filenames across repositories stay distinct.
pub(crate) fn qualify_path(repo: &str, path: &str) -> String {
    format!("{repo}:{path}")
}

/// Clip a body string to `max_bytes` on a UTF-8 char boundary.
fn truncate_body(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// True when an opened path is the same inspection entry, with or without the repo prefix.
pub(crate) fn inspect_paths_match(identified: &str, opened: &str) -> bool {
    if identified == opened {
        return true;
    }
    let id_path = identified
        .split_once(':')
        .map(|(_, p)| p)
        .unwrap_or(identified);
    let open_path = opened.split_once(':').map(|(_, p)| p).unwrap_or(opened);
    id_path == open_path
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

fn request_matches_config(
    needle: &str,
    rec: &crate::model::ConfigKeyRecord,
    docker_intent: bool,
) -> bool {
    if docker_intent && rec.key == "docker.extra_args" {
        return true;
    }
    let key = rec.key.to_lowercase();
    if needle.contains(&key) {
        return true;
    }
    if let Some(last) = key.rsplit('.').next() {
        if last.len() >= 8 && needle.contains(last) {
            return true;
        }
    }
    rec.value_tokens.iter().any(|tok| {
        let t = tok.to_lowercase();
        t.len() >= 8 && needle.contains(&t)
    })
}

fn docker_shipping_intent(needle: &str) -> bool {
    let docker = needle.contains("docker")
        || needle.contains("dockerfile")
        || needle.contains("extra_args")
        || needle.contains("build-context")
        || needle.contains("build-arg")
        || needle.contains("kellnr");
    let ship = (needle.contains("ship") || needle.contains("shipping"))
        && (needle.contains("binary") || needle.contains("rust"));
    docker || ship
}

fn dockerfile_beside(dpk_path: &str) -> String {
    match dpk_path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/Dockerfile"),
        None => "Dockerfile".into(),
    }
}

pub(crate) fn request_tokens(needle: &str) -> BTreeSet<String> {
    needle
        .split(|c: char| {
            !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
        })
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_string())
        .collect()
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
    if let Some((repo, spec)) = target.split_once(':') {
        if !spec.is_empty() && repo_names(store)?.iter().any(|n| n == repo) {
            return Ok((repo.to_string(), spec.to_string()));
        }
    }
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

fn semantic_issues(
    store: &BinderStore,
    names: &[String],
    match_tokens: &BTreeSet<String>,
) -> Result<Vec<IssueRef>> {
    if match_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut scored: Vec<(usize, IssueRef)> = Vec::new();
    for name in names {
        let q = QueryEngine::open(store, name);
        for issue in q.issues()? {
            let overlap = issue
                .tokens
                .iter()
                .filter(|t| match_tokens.contains(t.as_str()))
                .count();
            if overlap >= 2 {
                scored.push((
                    overlap,
                    IssueRef {
                        number: issue.number,
                        title: issue.title.clone(),
                        url: issue.url,
                        state: issue.state,
                        repo: name.clone(),
                        body_excerpt: truncate_body(&issue.body, 1000),
                    },
                ));
            }
        }
    }
    scored.sort_by_key(|a| Reverse(a.0));
    scored.truncate(10);
    Ok(scored.into_iter().map(|(_, r)| r).collect())
}

fn semantic_prs(
    store: &BinderStore,
    names: &[String],
    match_tokens: &BTreeSet<String>,
) -> Result<Vec<PrRef>> {
    if match_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut scored: Vec<(usize, PrRef)> = Vec::new();
    for name in names {
        let q = QueryEngine::open(store, name);
        for pr in q.prs()? {
            let overlap = pr
                .tokens
                .iter()
                .filter(|t| match_tokens.contains(t.as_str()))
                .count();
            if overlap >= 2 {
                scored.push((
                    overlap,
                    PrRef {
                        number: pr.number,
                        title: pr.title,
                        url: pr.url,
                        state: pr.state,
                        repo: name.clone(),
                    },
                ));
            }
        }
    }
    scored.sort_by_key(|a| Reverse(a.0));
    scored.truncate(8);
    Ok(scored.into_iter().map(|(_, r)| r).collect())
}

fn semantic_docs(
    store: &BinderStore,
    names: &[String],
    match_tokens: &BTreeSet<String>,
) -> Result<Vec<DocRef>> {
    if match_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut scored: Vec<(usize, DocRef)> = Vec::new();
    for name in names {
        let q = QueryEngine::open(store, name);
        for doc in q.docs()? {
            let overlap = doc
                .tokens
                .iter()
                .filter(|t| match_tokens.contains(t.as_str()))
                .count();
            if overlap >= 2 {
                scored.push((
                    overlap,
                    DocRef {
                        path: doc.path,
                        title: doc.title,
                        repo: name.clone(),
                        kind: doc.kind,
                    },
                ));
            }
        }
    }
    scored.sort_by_key(|a| Reverse(a.0));
    scored.truncate(8);
    Ok(scored.into_iter().map(|(_, r)| r).collect())
}

fn semantic_guidance(
    store: &BinderStore,
    names: &[String],
    match_tokens: &BTreeSet<String>,
    anchor_repos: &BTreeSet<&str>,
) -> Result<Vec<GuidanceItem>> {
    if match_tokens.is_empty() {
        return Ok(Vec::new());
    }
    // Iterate repo-specific names plus "" for cross-repo (stored under code://question/cross/).
    let cross = String::new();
    let all_names = names.iter().chain(std::iter::once(&cross));
    let mut scored: Vec<(usize, GuidanceItem)> = Vec::new();
    for name in all_names {
        let q = QueryEngine::open(store, name.as_str());
        for rec in q.questions()? {
            let repo_match = rec.repo.is_empty()
                || anchor_repos.is_empty()
                || anchor_repos.contains(rec.repo.as_str());
            if !repo_match {
                continue;
            }
            let threshold = if rec.repo.is_empty() { 2 } else { 1 };
            let overlap = rec
                .tags
                .iter()
                .filter(|t| match_tokens.contains(t.as_str()))
                .count();
            if overlap >= threshold {
                scored.push((
                    overlap,
                    GuidanceItem {
                        id: rec.id,
                        question: rec.question,
                        answer: rec.answer,
                    },
                ));
            }
        }
    }
    scored.sort_by_key(|a| Reverse(a.0));
    scored.dedup_by_key(|a| a.1.id.clone());
    scored.truncate(5);
    Ok(scored.into_iter().map(|(_, r)| r).collect())
}

fn compute_recent_changes(
    store: &BinderStore,
    repos: &BTreeSet<String>,
    file_set: &[String],
    since_sha: &str,
) -> Vec<DriftedFile> {
    let mut out = Vec::new();
    for repo_name in repos {
        let q = QueryEngine::open(store, repo_name);
        let head_sha = match q.bookmark() {
            Ok(Some(b)) => b.sha,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("codeintel: bookmark failed for {repo_name}: {e}");
                continue;
            }
        };
        if head_sha == since_sha {
            continue;
        }
        let repo_prefix = format!("{repo_name}:");
        let drift = match crate::drift::analyze(store, repo_name, since_sha, &head_sha) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("codeintel: drift analyze failed for {repo_name} since {since_sha}: {e}");
                continue;
            }
        };
        for path in drift.files_added {
            if file_set.iter().any(|e| {
                e.strip_prefix(repo_prefix.as_str())
                    .is_some_and(|p| p == path)
            }) {
                out.push(DriftedFile {
                    path,
                    repo: repo_name.clone(),
                    change: "added".into(),
                });
            }
        }
        for path in drift.files_removed {
            if file_set.iter().any(|e| {
                e.strip_prefix(repo_prefix.as_str())
                    .is_some_and(|p| p == path)
            }) {
                out.push(DriftedFile {
                    path,
                    repo: repo_name.clone(),
                    change: "removed".into(),
                });
            }
        }
    }
    out
}
