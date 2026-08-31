use crate::doctrine::LayerDoctrine;
use crate::error::Result;
use crate::ids::{
    prefix_config, prefix_crate, prefix_doc, prefix_file, prefix_issue, prefix_mod, prefix_pr,
    prefix_question, prefix_sym, REL_CALLS, REL_DEPENDS_ON, REL_IMPLEMENTS, REL_REFERENCES,
};
use crate::indexer::decode_obj;
use crate::model::{
    ConfigKeyRecord, CrateRecord, DocRecord, FileRecord, IssueRecord, ModuleRecord,
    PullRequestRecord, QuestionRecord, SymbolKind, SymbolRecord, SyncBookmark, Visibility,
};
use bindb::BinderStore;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

pub struct QueryEngine<'a> {
    store: &'a BinderStore,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryTimed<T: Serialize> {
    pub latency_ms: u64,
    pub result: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cycle {
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayerViolation {
    pub from_crate: String,
    pub to_crate: String,
    pub from_rank: u32,
    pub to_rank: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnusedApi {
    pub qualname: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub structural_hash: String,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FanStat {
    pub qualname: String,
    pub fan_in: usize,
    pub fan_out: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    pub path: String,
    pub loc: u32,
    pub unwrap_count: u32,
    pub fan_in: usize,
    pub fan_out: usize,
    pub score: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatternHit {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchitectureSummary {
    pub repository: String,
    pub sha: Option<String>,
    pub crates: usize,
    pub files: usize,
    pub rust_files: usize,
    pub symbols: usize,
    pub public_symbols: usize,
    pub modules: usize,
    pub doctrine: String,
}

impl<'a> QueryEngine<'a> {
    pub fn open(store: &'a BinderStore, name: impl Into<String>) -> Self {
        Self {
            store,
            name: name.into(),
        }
    }

    pub(crate) fn store(&self) -> &'a BinderStore {
        self.store
    }

    pub fn timed<T: Serialize>(&self, f: impl FnOnce() -> Result<T>) -> Result<QueryTimed<T>> {
        let t = Instant::now();
        let result = f()?;
        Ok(QueryTimed {
            latency_ms: t.elapsed().as_millis() as u64,
            result,
        })
    }

    pub fn bookmark(&self) -> Result<Option<SyncBookmark>> {
        match self.store.get(&crate::ids::sync_id(&self.name))? {
            Some(v) => Ok(Some(decode_obj(&v)?)),
            None => Ok(None),
        }
    }

    pub fn crate_dep_graph(&self) -> Result<Vec<(String, String)>> {
        let crates = self.crates()?;
        let mut edges = Vec::new();
        for c in &crates {
            let id = crate::ids::crate_id(&self.name, &c.name);
            for e in self.store.edges_from_with_type(&id, REL_DEPENDS_ON)? {
                if let Some(to) = e.to_object {
                    if let Some(rec) = self.store.get(&to)? {
                        let t: CrateRecord = decode_obj(&rec)?;
                        edges.push((c.name.clone(), t.name));
                    }
                }
            }
        }
        Ok(edges)
    }

    pub fn module_dep_graph(&self) -> Result<Vec<(String, String)>> {
        let mods = self.modules()?;
        let mut edges = Vec::new();
        for m in &mods {
            let id = crate::ids::module_id(&self.name, &m.path);
            for e in self.store.edges_from_with_type(&id, REL_DEPENDS_ON)? {
                if let Some(to) = e.to_object {
                    if let Some(rec) = self.store.get(&to)? {
                        let t: ModuleRecord = decode_obj(&rec)?;
                        edges.push((m.path.clone(), t.path));
                    }
                }
            }
        }
        Ok(edges)
    }

    pub fn circular_dependencies(&self) -> Result<Vec<Cycle>> {
        let edges = self.crate_dep_graph()?;
        Ok(crate_cycles(&edges))
    }

    pub fn layering_violations(&self) -> Result<Vec<LayerViolation>> {
        let crates = self.crates()?;
        let names: Vec<String> = crates.iter().map(|c| c.name.clone()).collect();
        let doctrine = LayerDoctrine::detect(&names);
        let mut out = Vec::new();
        for (from, to) in self.crate_dep_graph()? {
            if doctrine.violates(&from, &to) {
                out.push(LayerViolation {
                    from_rank: doctrine.rank_of(&from).unwrap_or(0),
                    to_rank: doctrine.rank_of(&to).unwrap_or(0),
                    from_crate: from,
                    to_crate: to,
                });
            }
        }
        Ok(out)
    }

    pub fn unused_public_apis(&self) -> Result<Vec<UnusedApi>> {
        let symbols = self.symbols()?;
        let mut fan_in: HashMap<String, usize> = HashMap::new();
        for s in &symbols {
            let id = crate::ids::symbol_id(&self.name, &s.qualname);
            let usage = self
                .store
                .edges_to(&id)?
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.relationship_type.as_deref(),
                        Some(REL_CALLS) | Some(REL_REFERENCES)
                    )
                })
                .count();
            fan_in.insert(s.qualname.clone(), usage);
        }
        Ok(symbols
            .into_iter()
            .filter(|s| {
                s.visibility == Visibility::Public
                    && !s.in_test
                    && s.kind != SymbolKind::Module
                    && fan_in.get(&s.qualname).copied().unwrap_or(0) == 0
                    && s.name != "main"
            })
            .map(|s| UnusedApi {
                qualname: s.qualname,
                kind: s.kind,
                file_path: s.file_path,
                visibility: s.visibility,
            })
            .collect())
    }

    pub fn duplicates(&self) -> Result<Vec<DuplicateGroup>> {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for s in self.symbols()? {
            if !matches!(s.kind, SymbolKind::Function | SymbolKind::Method) {
                continue;
            }
            if s.loc < 4 {
                continue;
            }
            if let Some(h) = s.structural_hash {
                groups.entry(h).or_default().push(s.qualname);
            }
        }
        Ok(groups
            .into_iter()
            .filter(|(_, m)| m.len() > 1)
            .map(|(structural_hash, members)| DuplicateGroup {
                structural_hash,
                members,
            })
            .collect())
    }

    pub fn fan_stats(&self, limit: usize) -> Result<Vec<FanStat>> {
        let mut stats: Vec<FanStat> = Vec::new();
        for s in self.symbols()? {
            let id = crate::ids::symbol_id(&self.name, &s.qualname);
            let fan_in = self.store.edges_to(&id)?.len();
            let fan_out = self.store.edges_from_with_type(&id, REL_CALLS)?.len();
            stats.push(FanStat {
                qualname: s.qualname,
                fan_in,
                fan_out,
            });
        }
        stats.sort_by_key(|s| std::cmp::Reverse(s.fan_in + s.fan_out));
        stats.truncate(limit);
        Ok(stats)
    }

    pub fn impact(&self, qualname: &str) -> Result<Vec<String>> {
        let id = crate::ids::symbol_id(&self.name, qualname);
        let mut out = Vec::new();
        for e in self.store.edges_to(&id)? {
            if let Some(from) = e.from_object {
                out.push(from.0);
            }
        }
        Ok(out)
    }

    pub fn callers(&self, qualname: &str) -> Result<Vec<String>> {
        let id = crate::ids::symbol_id(&self.name, qualname);
        let mut out = Vec::new();
        for e in self.store.edges_to(&id)? {
            if e.relationship_type.as_deref() == Some(REL_CALLS) {
                if let Some(from) = e.from_object {
                    out.push(from.0);
                }
            }
        }
        Ok(out)
    }

    pub fn trait_impls(&self, trait_qual: &str) -> Result<Vec<String>> {
        let id = crate::ids::symbol_id(&self.name, trait_qual);
        let mut out = Vec::new();
        for e in self.store.edges_to(&id)? {
            if e.relationship_type.as_deref() == Some(REL_IMPLEMENTS) {
                if let Some(from) = e.from_object {
                    out.push(from.0);
                }
            }
        }
        Ok(out)
    }

    pub fn paths(&self, from_mod: &str, to_mod: &str) -> Result<Vec<Vec<String>>> {
        let edges = self.module_dep_graph()?;
        Ok(paths_between(&edges, from_mod, to_mod, 8))
    }

    pub fn hotspots(&self, limit: usize) -> Result<Vec<Hotspot>> {
        let files = self.files()?;
        let mut out = Vec::new();
        for f in files {
            let id = crate::ids::file_id(&self.name, &f.path);
            let fan_in = self.store.edges_to(&id)?.len();
            let fan_out = self.store.edges_from(&id)?.len();
            let score = (fan_in.saturating_add(1))
                * (fan_out.saturating_add(1))
                * (f.loc as usize).saturating_add(1);
            out.push(Hotspot {
                path: f.path,
                loc: f.loc,
                unwrap_count: f.unwrap_count,
                fan_in,
                fan_out,
                score,
            });
        }
        out.sort_by_key(|h| std::cmp::Reverse(h.score));
        out.truncate(limit);
        Ok(out)
    }

    pub fn patterns(&self) -> Result<Vec<PatternHit>> {
        let mut hits = Vec::new();
        let mut methods_by_parent: HashMap<String, Vec<SymbolRecord>> = HashMap::new();
        for s in self.symbols()? {
            if s.kind == SymbolKind::Method {
                if let Some(p) = &s.parent_symbol {
                    methods_by_parent
                        .entry(p.clone())
                        .or_default()
                        .push(s.clone());
                }
            }
            if s.kind == SymbolKind::Struct && s.loc <= 8 {
                hits.push(PatternHit {
                    kind: "possible_newtype".into(),
                    detail: s.qualname,
                });
            }
        }
        for (parent, methods) in methods_by_parent {
            let builders = methods
                .iter()
                .filter(|m| m.name != "new" && m.name != "build")
                .count();
            if builders >= 3 {
                hits.push(PatternHit {
                    kind: "builder_like".into(),
                    detail: format!("{parent} ({builders} methods)"),
                });
            }
        }
        Ok(hits)
    }

    pub fn antipatterns(&self) -> Result<Vec<PatternHit>> {
        let mut hits = Vec::new();
        for f in self.files()? {
            if f.unwrap_count > 5 && !f.path.contains("/tests/") {
                hits.push(PatternHit {
                    kind: "unwrap_heavy".into(),
                    detail: format!("{} unwrap/expect calls in {}", f.unwrap_count, f.path),
                });
            }
            if f.wildcard_imports > 0 {
                hits.push(PatternHit {
                    kind: "wildcard_import".into(),
                    detail: f.path.clone(),
                });
            }
            if f.loc > 800 {
                hits.push(PatternHit {
                    kind: "god_file".into(),
                    detail: format!("{} ({} loc)", f.path, f.loc),
                });
            }
        }
        for s in self.symbols()? {
            if s.complexity > 20 && !s.in_test {
                hits.push(PatternHit {
                    kind: "high_complexity".into(),
                    detail: format!("{} complexity {}", s.qualname, s.complexity),
                });
            }
        }
        Ok(hits)
    }

    pub fn mermaid_crate_graph(&self) -> Result<String> {
        let mut lines = vec!["graph LR".to_string()];
        for (from, to) in self.crate_dep_graph()? {
            lines.push(format!("  {from} --> {to}"));
        }
        Ok(lines.join("\n"))
    }

    pub fn summary(&self) -> Result<ArchitectureSummary> {
        let files = self.files()?;
        let rust_files = files.iter().filter(|f| f.language == "rust").count();
        let symbols = self.symbols()?;
        let public_symbols = symbols
            .iter()
            .filter(|s| s.visibility == Visibility::Public)
            .count();
        let crates = self.crates()?;
        let names: Vec<String> = crates.iter().map(|c| c.name.clone()).collect();
        Ok(ArchitectureSummary {
            repository: self.name.clone(),
            sha: self.bookmark()?.map(|b| b.sha),
            crates: crates.len(),
            files: files.len(),
            rust_files,
            symbols: symbols.len(),
            public_symbols,
            modules: self.modules()?.len(),
            doctrine: LayerDoctrine::detect(&names).name,
        })
    }

    pub fn files(&self) -> Result<Vec<FileRecord>> {
        decode_list(self.store, &prefix_file(&self.name))
    }

    pub fn symbols(&self) -> Result<Vec<SymbolRecord>> {
        decode_list(self.store, &prefix_sym(&self.name))
    }

    pub fn modules(&self) -> Result<Vec<ModuleRecord>> {
        decode_list(self.store, &prefix_mod(&self.name))
    }

    pub fn crates(&self) -> Result<Vec<CrateRecord>> {
        decode_list(self.store, &prefix_crate(&self.name))
    }

    pub fn config_keys(&self) -> Result<Vec<ConfigKeyRecord>> {
        decode_list(self.store, &prefix_config(&self.name))
    }

    pub fn issues(&self) -> Result<Vec<IssueRecord>> {
        decode_list(self.store, &prefix_issue(&self.name))
    }

    pub fn prs(&self) -> Result<Vec<PullRequestRecord>> {
        decode_list(self.store, &prefix_pr(&self.name))
    }

    pub fn docs(&self) -> Result<Vec<DocRecord>> {
        decode_list(self.store, &prefix_doc(&self.name))
    }

    pub fn questions(&self) -> Result<Vec<QuestionRecord>> {
        decode_list(self.store, &prefix_question(&self.name))
    }
}

fn decode_list<T: serde::de::DeserializeOwned>(
    store: &BinderStore,
    prefix: &str,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    for v in store.list(prefix)? {
        out.push(decode_obj(&v)?);
    }
    Ok(out)
}

pub(crate) fn crate_cycles(edges: &[(String, String)]) -> Vec<Cycle> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut nodes = HashSet::new();
    for (a, b) in edges {
        adj.entry(a.as_str()).or_default().push(b.as_str());
        nodes.insert(a.as_str());
        nodes.insert(b.as_str());
    }
    let mut cycles = Vec::new();
    let mut stack = Vec::new();
    let mut on_stack = HashSet::new();
    let mut seen = HashSet::new();
    for n in nodes {
        dfs_cycles(n, &adj, &mut stack, &mut on_stack, &mut seen, &mut cycles);
    }
    cycles.sort_by_key(|a| a.nodes.len());
    cycles.dedup_by(|a, b| a.nodes == b.nodes);
    cycles
}

fn dfs_cycles<'a>(
    node: &'a str,
    adj: &HashMap<&str, Vec<&'a str>>,
    stack: &mut Vec<&'a str>,
    on_stack: &mut HashSet<&'a str>,
    seen: &mut HashSet<&'a str>,
    cycles: &mut Vec<Cycle>,
) {
    if !seen.insert(node) {
        return;
    }
    stack.push(node);
    on_stack.insert(node);
    if let Some(nexts) = adj.get(node) {
        for n in nexts {
            if on_stack.contains(n) {
                if let Some(i) = stack.iter().position(|x| x == n) {
                    let mut c: Vec<String> = stack[i..].iter().map(|s| (*s).to_string()).collect();
                    c.push((*n).to_string());
                    cycles.push(Cycle { nodes: c });
                }
            } else {
                dfs_cycles(n, adj, stack, on_stack, seen, cycles);
            }
        }
    }
    on_stack.remove(node);
    stack.pop();
}

fn paths_between(
    edges: &[(String, String)],
    from: &str,
    to: &str,
    max_depth: usize,
) -> Vec<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in edges {
        adj.entry(a.as_str()).or_default().push(b.as_str());
    }
    let mut found = Vec::new();
    let mut q: VecDeque<Vec<&str>> = VecDeque::new();
    q.push_back(vec![from]);
    while let Some(path) = q.pop_front() {
        if path.len() > max_depth {
            continue;
        }
        let last = *path.last().unwrap();
        if last == to && path.len() > 1 {
            found.push(path.iter().map(|s| (*s).to_string()).collect());
            continue;
        }
        if let Some(nexts) = adj.get(last) {
            for n in nexts {
                if path.contains(n) {
                    continue;
                }
                let mut p = path.clone();
                p.push(n);
                q.push_back(p);
            }
        }
    }
    found
}
