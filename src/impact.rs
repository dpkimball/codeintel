use crate::error::{CodeintelError, Result};
use crate::ids::{
    crate_id, file_id, module_id, symbol_id, REL_CALLS, REL_IMPLEMENTS, REL_REFERENCES,
};
use crate::indexer::decode_obj;
use crate::model::{CrateRecord, FileRecord, ModuleRecord, SymbolRecord};
use crate::query::QueryEngine;
use bindb::ObjectId;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Crate,
    Module,
    File,
    Symbol,
}

#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub rel: String,
    pub object_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InspectionSet {
    pub crates: Vec<String>,
    pub modules: Vec<String>,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpactReport {
    pub target: String,
    pub target_kind: TargetKind,
    pub object_id: String,
    pub primary_source: &'static str,
    pub source_files_opened: usize,
    pub incoming: Vec<Neighbor>,
    pub outgoing: Vec<Neighbor>,
    pub callers: Vec<Neighbor>,
    pub referenced_by: Vec<Neighbor>,
    pub references: Vec<Neighbor>,
    pub trait_impls: Vec<Neighbor>,
    pub downstream_modules: Vec<String>,
    pub transitive_crate_dependents: Vec<String>,
    pub inspection_set: InspectionSet,
}

struct Resolved {
    kind: TargetKind,
    spec: String,
    id: ObjectId,
}

/// Change impact from bindb only. Paths in the report are payload fields, not file reads.
pub fn analyze(engine: &QueryEngine, spec: &str) -> Result<ImpactReport> {
    let resolved = resolve(engine, spec)?;
    let mut incoming = neighbors(engine, &resolved.id, true)?;
    let mut outgoing = neighbors(engine, &resolved.id, false)?;
    incoming.sort_by(|a, b| a.label.cmp(&b.label));
    outgoing.sort_by(|a, b| a.label.cmp(&b.label));

    let callers = typed_neighbors(engine, &resolved.id, REL_CALLS, true)?;
    let referenced_by = typed_neighbors(engine, &resolved.id, REL_REFERENCES, true)?;
    let references = typed_neighbors(engine, &resolved.id, REL_REFERENCES, false)?;
    let trait_impls = typed_neighbors(engine, &resolved.id, REL_IMPLEMENTS, true)?;

    let downstream_modules = match resolved.kind {
        TargetKind::Module => reverse_reachable(engine.module_dep_graph()?, &resolved.spec),
        TargetKind::Symbol => modules_of_symbols(engine, &callers)?,
        TargetKind::File => modules_of_file(engine, &resolved.spec)?,
        TargetKind::Crate => Vec::new(),
    };

    let crate_name = crate_of(engine, &resolved)?;
    let transitive_crate_dependents = if let Some(c) = &crate_name {
        reverse_reachable(engine.crate_dep_graph()?, c)
    } else {
        Vec::new()
    };

    let inspection_set = build_inspection_set(InspectionInput {
        engine,
        resolved: &resolved,
        callers: &callers,
        referenced_by: &referenced_by,
        trait_impls: &trait_impls,
        downstream_modules: &downstream_modules,
        transitive_crates: &transitive_crate_dependents,
        crate_name: crate_name.as_deref(),
    })?;

    Ok(ImpactReport {
        target: resolved.spec,
        target_kind: resolved.kind,
        object_id: resolved.id.0,
        primary_source: "bindb",
        source_files_opened: 0,
        incoming,
        outgoing,
        callers,
        referenced_by,
        references,
        trait_impls,
        downstream_modules,
        transitive_crate_dependents,
        inspection_set,
    })
}

fn resolve(engine: &QueryEngine, spec: &str) -> Result<Resolved> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(CodeintelError::Other("impact --target is required".into()));
    }
    for c in engine.crates()? {
        if c.name == spec {
            return Ok(Resolved {
                kind: TargetKind::Crate,
                spec: c.name,
                id: crate_id(&engine.name, spec),
            });
        }
    }
    for f in engine.files()? {
        if f.path == spec {
            return Ok(Resolved {
                kind: TargetKind::File,
                spec: f.path,
                id: file_id(&engine.name, spec),
            });
        }
    }
    for m in engine.modules()? {
        if m.path == spec {
            return Ok(Resolved {
                kind: TargetKind::Module,
                spec: m.path,
                id: module_id(&engine.name, spec),
            });
        }
    }
    for s in engine.symbols()? {
        if s.qualname == spec {
            return Ok(Resolved {
                kind: TargetKind::Symbol,
                spec: s.qualname,
                id: symbol_id(&engine.name, spec),
            });
        }
    }
    for s in engine.symbols()? {
        if s.qualname.ends_with(spec) || s.name == spec {
            return Ok(Resolved {
                kind: TargetKind::Symbol,
                spec: s.qualname.clone(),
                id: symbol_id(&engine.name, &s.qualname),
            });
        }
    }
    Err(CodeintelError::Other(format!(
        "no crate, module, file, or symbol matching {spec:?} in bindb"
    )))
}

fn neighbors(engine: &QueryEngine, id: &ObjectId, incoming: bool) -> Result<Vec<Neighbor>> {
    let edges = if incoming {
        engine.store().edges_to(id)?
    } else {
        engine.store().edges_from(id)?
    };
    let mut out = Vec::new();
    for e in edges {
        let other = if incoming { e.from_object } else { e.to_object };
        let Some(other) = other else { continue };
        let rel = e.relationship_type.unwrap_or_default();
        let label = label_of(engine, &other)?;
        out.push(Neighbor {
            rel,
            object_id: other.0,
            label,
        });
    }
    Ok(out)
}

fn typed_neighbors(
    engine: &QueryEngine,
    id: &ObjectId,
    rel: &str,
    incoming: bool,
) -> Result<Vec<Neighbor>> {
    Ok(neighbors(engine, id, incoming)?
        .into_iter()
        .filter(|n| n.rel == rel)
        .collect())
}

fn label_of(engine: &QueryEngine, id: &ObjectId) -> Result<String> {
    let Some(v) = engine.store().get(id)? else {
        return Ok(id.0.clone());
    };
    if id.0.contains("/crate/") {
        if let Ok(c) = decode_obj::<CrateRecord>(&v) {
            return Ok(c.name);
        }
    }
    if id.0.contains("/mod/") {
        if let Ok(m) = decode_obj::<ModuleRecord>(&v) {
            return Ok(m.path);
        }
    }
    if id.0.contains("/file/") {
        if let Ok(f) = decode_obj::<FileRecord>(&v) {
            return Ok(f.path);
        }
    }
    if id.0.contains("/sym/") {
        if let Ok(s) = decode_obj::<SymbolRecord>(&v) {
            return Ok(s.qualname);
        }
    }
    Ok(id.0.clone())
}

fn crate_of(engine: &QueryEngine, resolved: &Resolved) -> Result<Option<String>> {
    match resolved.kind {
        TargetKind::Crate => Ok(Some(resolved.spec.clone())),
        TargetKind::Module => {
            if let Some(v) = engine.store().get(&resolved.id)? {
                let m: ModuleRecord = decode_obj(&v)?;
                return Ok(Some(m.crate_name).filter(|s| !s.is_empty()));
            }
            Ok(None)
        }
        TargetKind::File => {
            if let Some(v) = engine.store().get(&resolved.id)? {
                let f: FileRecord = decode_obj(&v)?;
                return Ok(f.crate_name.filter(|s| !s.is_empty()));
            }
            Ok(None)
        }
        TargetKind::Symbol => {
            if let Some(v) = engine.store().get(&resolved.id)? {
                let s: SymbolRecord = decode_obj(&v)?;
                return Ok(Some(s.crate_name).filter(|s| !s.is_empty()));
            }
            Ok(None)
        }
    }
}

fn reverse_reachable(edges: Vec<(String, String)>, start: &str) -> Vec<String> {
    let mut rev: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in &edges {
        rev.entry(b.as_str()).or_default().push(a.as_str());
    }
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    q.push_back(start);
    seen.insert(start.to_string());
    let mut out = Vec::new();
    while let Some(n) = q.pop_front() {
        if let Some(preds) = rev.get(n) {
            for p in preds {
                if seen.insert((*p).to_string()) {
                    out.push((*p).to_string());
                    q.push_back(p);
                }
            }
        }
    }
    out.sort();
    out
}

fn modules_of_symbols(engine: &QueryEngine, callers: &[Neighbor]) -> Result<Vec<String>> {
    let mut mods = BTreeSet::new();
    for n in callers {
        let id = ObjectId::new(n.object_id.clone());
        if let Some(v) = engine.store().get(&id)? {
            if let Ok(s) = decode_obj::<SymbolRecord>(&v) {
                if !s.module.is_empty() {
                    mods.insert(s.module);
                }
            }
        }
    }
    Ok(mods.into_iter().collect())
}

fn modules_of_file(engine: &QueryEngine, path: &str) -> Result<Vec<String>> {
    let mut mods = BTreeSet::new();
    for f in engine.files()? {
        if f.path == path {
            if let Some(m) = f.module {
                mods.insert(m);
            }
        }
    }
    Ok(mods.into_iter().collect())
}

struct InspectionInput<'a> {
    engine: &'a QueryEngine<'a>,
    resolved: &'a Resolved,
    callers: &'a [Neighbor],
    referenced_by: &'a [Neighbor],
    trait_impls: &'a [Neighbor],
    downstream_modules: &'a [String],
    transitive_crates: &'a [String],
    crate_name: Option<&'a str>,
}

fn build_inspection_set(input: InspectionInput<'_>) -> Result<InspectionSet> {
    let InspectionInput {
        engine,
        resolved,
        callers,
        referenced_by,
        trait_impls,
        downstream_modules,
        transitive_crates,
        crate_name,
    } = input;
    let mut crates = BTreeSet::new();
    let mut modules = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut symbols = BTreeSet::new();

    if let Some(c) = crate_name {
        crates.insert(c.to_string());
    }
    for c in transitive_crates {
        crates.insert(c.clone());
    }
    for m in downstream_modules {
        modules.insert(m.clone());
    }

    match resolved.kind {
        TargetKind::Symbol => {
            symbols.insert(resolved.spec.clone());
            if let Some(v) = engine.store().get(&resolved.id)? {
                if let Ok(s) = decode_obj::<SymbolRecord>(&v) {
                    if !s.file_path.is_empty() {
                        files.insert(s.file_path);
                    }
                    if !s.module.is_empty() {
                        modules.insert(s.module);
                    }
                }
            }
        }
        TargetKind::File => {
            files.insert(resolved.spec.clone());
        }
        TargetKind::Module => {
            modules.insert(resolved.spec.clone());
            for f in engine.files()? {
                if f.module.as_deref() == Some(resolved.spec.as_str()) {
                    files.insert(f.path);
                }
            }
        }
        TargetKind::Crate => {
            for f in engine.files()? {
                if f.crate_name.as_deref() == Some(resolved.spec.as_str())
                    && is_crate_inspect_file(&f.path)
                {
                    files.insert(f.path);
                    if let Some(m) = f.module {
                        modules.insert(m);
                    }
                }
            }
        }
    }

    for n in callers.iter().chain(referenced_by).chain(trait_impls) {
        symbols.insert(n.label.clone());
        let id = ObjectId::new(n.object_id.clone());
        if let Some(v) = engine.store().get(&id)? {
            if let Ok(s) = decode_obj::<SymbolRecord>(&v) {
                if !s.file_path.is_empty() {
                    files.insert(s.file_path);
                }
                if !s.module.is_empty() {
                    modules.insert(s.module);
                }
                if !s.crate_name.is_empty() {
                    crates.insert(s.crate_name);
                }
            }
        }
    }

    Ok(InspectionSet {
        crates: crates.into_iter().collect(),
        modules: modules.into_iter().collect(),
        files: files.into_iter().collect(),
        symbols: symbols.into_iter().collect(),
    })
}

/// Crate-wide plans name architecture files, not repo clutter under the crate root.
pub(crate) fn is_crate_inspect_file(path: &str) -> bool {
    let p = path.replace('\\', "/");
    if p.split('/').any(|s| s == ".github") {
        return false;
    }
    if p.ends_with(".rs") {
        return true;
    }
    let name = p.rsplit('/').next().unwrap_or("");
    matches!(
        name,
        "Cargo.toml" | "dpk.toml" | "Dockerfile" | "Makefile" | "CLAUDE.md" | "README.md"
    ) || p.ends_with(".cargo/config.toml")
}
