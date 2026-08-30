use crate::cargo_graph::{rust_module_path, scan_workspace, WorkspaceGraph};
use crate::config_keys::{extract_dpk_toml, extract_env_manifest, is_dpk_toml, is_env_manifest};
use crate::error::{CodeintelError, Result};
use crate::git_sync::{self, FileDelta, GitHead, TrackedFile};
use crate::ids::{
    config_id, crate_id, doc_id, edge_id, file_id, module_id, repo_id, symbol_id, sync_id, ACTOR,
    NAMESPACE, REL_AT_COMMIT, REL_CALLS, REL_CONTAINS, REL_DEFINED_IN, REL_DEPENDS_ON,
    REL_IMPLEMENTS, REL_IMPORTS, REL_IN_REPO, REL_PARENT_OF, REL_REFERENCES,
};
use crate::model::{
    CommitRecord, CrateRecord, DocRecord, FileRecord, ModuleRecord, RepoRecord, SymbolRecord,
    SyncBookmark,
};
use crate::parser::{detect_language, FileIr, LanguageParser};
use crate::python_parser::{python_module_path, PythonParser};
use crate::rust_parser::RustParser;
use crate::ts_parser::{ts_module_path, TypeScriptParser};
use bindb::{BinderStore, ObjectId, Provenance, ProvenanceSource, Transaction};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct IndexOptions {
    pub repo_root: PathBuf,
    pub store_path: PathBuf,
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStats {
    pub name: String,
    pub sha: String,
    pub branch: String,
    pub skipped: bool,
    pub files_seen: u64,
    pub files_indexed: u64,
    pub skipped_unchanged: u64,
    pub symbol_count: u64,
    pub source_bytes: u64,
    pub model_bytes: u64,
    pub duration_ms: u64,
    pub store_bytes: u64,
}

pub fn index(opts: IndexOptions) -> Result<IndexStats> {
    let name = opts.name.clone().unwrap_or_else(|| {
        opts.repo_root
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .or_else(|| {
                opts.repo_root
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "repo".into())
    });
    std::fs::create_dir_all(&opts.store_path)?;
    let store = BinderStore::open(&opts.store_path)?;
    store.register_namespace(NAMESPACE, ACTOR)?;
    let mut stats = index_into(&store, &opts.repo_root, &name)?;
    stats.store_bytes = dir_size(&opts.store_path)?;
    store.close()?;
    Ok(stats)
}

pub fn index_into(store: &BinderStore, requested: &Path, name: &str) -> Result<IndexStats> {
    let started = Instant::now();
    let scope = git_sync::index_scope(requested)?;
    let head = git_sync::head(&scope.git_top)?;
    let tree = git_sync::ls_tree_scoped(&scope, &head.sha)?;
    let bookmark = load_bookmark(store, name)?;

    if let Some(prev) = &bookmark {
        if prev.sha == head.sha {
            return Ok(IndexStats {
                name: name.to_string(),
                sha: head.sha,
                branch: head.branch,
                skipped: true,
                files_seen: tree.len() as u64,
                files_indexed: 0,
                skipped_unchanged: tree.len() as u64,
                symbol_count: prev.symbol_count,
                source_bytes: prev.source_bytes,
                model_bytes: prev.model_bytes,
                duration_ms: started.elapsed().as_millis() as u64,
                store_bytes: 0,
            });
        }
    }

    let old_tree = if let Some(prev) = &bookmark {
        git_sync::ls_tree_scoped(&scope, &prev.sha).unwrap_or_default()
    } else {
        Vec::new()
    };
    let deltas = git_sync::diff_trees(&old_tree, &tree);
    let ws = scan_workspace(requested)?;
    let provenance = git_provenance(&head);

    put_repo_and_commit(store, name, requested, &head, &provenance)?;

    let mut files_indexed = 0u64;
    let mut source_bytes = bookmark.as_ref().map(|b| b.source_bytes).unwrap_or(0);
    let mut parsed: Vec<(String, FileIr, String)> = Vec::new();

    for delta in &deltas {
        match delta {
            FileDelta::Deleted { path } => {
                tombstone_file(store, name, path, &provenance)?;
                files_indexed += 1;
            }
            FileDelta::Added(f) | FileDelta::Modified(f) => {
                if let Some((ir, bytes)) = index_one_file(&IndexFileCtx {
                    store,
                    workspace: requested,
                    scope: &scope,
                    name,
                    file: f,
                    ws: &ws,
                    head: &head,
                    provenance: &provenance,
                })? {
                    if bookmark.is_none() {
                        source_bytes = source_bytes.saturating_add(bytes as u64);
                    }
                    parsed.push((f.path.clone(), ir, f.blob_hash.clone()));
                    files_indexed += 1;
                } else {
                    files_indexed += 1;
                }
            }
        }
    }
    let skipped_unchanged = (tree.len() as u64).saturating_sub(
        deltas
            .iter()
            .filter(|d| !matches!(d, FileDelta::Deleted { .. }))
            .count() as u64,
    );

    index_crates(store, name, &ws, &provenance)?;
    resolve_relationships(store, name, &parsed, &provenance)?;

    let symbol_count = store.list(&crate::ids::prefix_sym(name))?.len() as u64;
    let model_bytes = estimate_model_bytes(store, name)?;
    let duration_ms = started.elapsed().as_millis() as u64;

    let bm = SyncBookmark {
        repository: name.to_string(),
        sha: head.sha.clone(),
        branch: head.branch.clone(),
        indexed_files: tree.len() as u64,
        parsed_files: parsed.len() as u64,
        skipped_unchanged,
        symbol_count,
        source_bytes,
        model_bytes,
        initial_index_ms: bookmark
            .as_ref()
            .and_then(|b| b.initial_index_ms)
            .or(Some(duration_ms)),
        last_incremental_ms: bookmark.as_ref().map(|_| duration_ms),
    };
    let mut tx = Transaction::new();
    put_json(&mut tx, sync_id(name), &bm, Some(provenance.clone()))?;
    store.commit_buffered(tx, ACTOR, format!("sync {name} {}", head.sha))?;
    store.flush()?;
    stamp_commit_catalog(store, name, &head, &tree, &ws, &provenance)?;
    crate::federation::link_cross_repo(store)?;

    Ok(IndexStats {
        name: name.to_string(),
        sha: head.sha,
        branch: head.branch,
        skipped: false,
        files_seen: tree.len() as u64,
        files_indexed,
        skipped_unchanged,
        symbol_count,
        source_bytes,
        model_bytes,
        duration_ms,
        store_bytes: 0,
    })
}

struct IndexFileCtx<'a> {
    store: &'a BinderStore,
    workspace: &'a Path,
    scope: &'a git_sync::GitScope,
    name: &'a str,
    file: &'a TrackedFile,
    ws: &'a WorkspaceGraph,
    head: &'a GitHead,
    provenance: &'a Provenance,
}

fn index_one_file(ctx: &IndexFileCtx<'_>) -> Result<Option<(FileIr, usize)>> {
    let store = ctx.store;
    let workspace = ctx.workspace;
    let scope = ctx.scope;
    let name = ctx.name;
    let file = ctx.file;
    let ws = ctx.ws;
    let head = ctx.head;
    let provenance = ctx.provenance;
    tombstone_file(store, name, &file.path, provenance)?;
    let blob = git_sync::git_blob_path(&scope.prefix, &file.path);
    let bytes = git_sync::show_file(&scope.git_top, &head.sha, &blob)?;
    let language = detect_language(&file.path);
    let abs = workspace.join(&file.path);
    let crate_info = ws.crate_for_file(&abs);
    let (crate_name, module, ir) = if language == "rust" {
        if let Ok(src) = std::str::from_utf8(&bytes) {
            let cname = crate_info
                .map(|c| c.name.clone())
                .unwrap_or_else(|| name.to_string());
            let src_dir = crate_info
                .map(|c| c.src_dir.clone())
                .unwrap_or_else(|| workspace.join("src"));
            let module = rust_module_path(&cname, &src_dir, &abs);
            let parser = RustParser {
                crate_name: cname.clone(),
                module_path: module.clone(),
            };
            let ir = parser.parse(&file.path, src);
            (Some(cname), Some(module), ir)
        } else {
            (
                crate_info.map(|c| c.name.clone()),
                None,
                FileIr {
                    language: language.into(),
                    parse_error: Some("not utf-8".into()),
                    ..FileIr::default()
                },
            )
        }
    } else if language == "python" {
        if let Ok(src) = std::str::from_utf8(&bytes) {
            let cname = crate_info
                .map(|c| c.name.clone())
                .unwrap_or_else(|| name.to_string());
            let mod_path = python_module_path(&file.path);
            let parser = PythonParser {
                crate_name: cname.clone(),
                module_path: mod_path.clone(),
            };
            let ir = parser.parse(&file.path, src);
            (Some(cname), Some(mod_path), ir)
        } else {
            (
                crate_info.map(|c| c.name.clone()),
                None,
                FileIr {
                    language: "python".into(),
                    parse_error: Some("not utf-8".into()),
                    ..FileIr::default()
                },
            )
        }
    } else if language == "typescript" {
        if let Ok(src) = std::str::from_utf8(&bytes) {
            let cname = crate_info
                .map(|c| c.name.clone())
                .unwrap_or_else(|| name.to_string());
            let mod_path = ts_module_path(&file.path);
            let parser = TypeScriptParser {
                crate_name: cname.clone(),
                module_path: mod_path.clone(),
            };
            let ir = parser.parse(&file.path, src);
            (Some(cname), Some(mod_path), ir)
        } else {
            (
                crate_info.map(|c| c.name.clone()),
                None,
                FileIr {
                    language: "typescript".into(),
                    parse_error: Some("not utf-8".into()),
                    ..FileIr::default()
                },
            )
        }
    } else {
        (
            crate_info.map(|c| c.name.clone()),
            None,
            FileIr {
                language: language.into(),
                loc: std::str::from_utf8(&bytes)
                    .map(|s| s.lines().count() as u32)
                    .unwrap_or(0),
                ..FileIr::default()
            },
        )
    };

    let fid = file_id(name, &file.path);
    let mut symbol_ids = Vec::new();
    let mut edge_ids = Vec::new();
    let mut config_ids = Vec::new();
    let mut tx = Transaction::new();

    if is_dpk_toml(&file.path) {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            for rec in extract_dpk_toml(&file.path, text) {
                let cid = config_id(name, &rec.file_path, &rec.key);
                config_ids.push(cid.0.clone());
                put_json(&mut tx, cid.clone(), &rec, Some(provenance.clone()))?;
                add_edge(&mut tx, &mut edge_ids, REL_CONTAINS, &fid, &cid, provenance);
            }
        }
    } else if let Ok(text) = std::str::from_utf8(&bytes) {
        if is_env_manifest(&file.path, text) {
            for rec in extract_env_manifest(&file.path, text) {
                let cid = config_id(name, &rec.file_path, &rec.key);
                config_ids.push(cid.0.clone());
                put_json(&mut tx, cid.clone(), &rec, Some(provenance.clone()))?;
                add_edge(&mut tx, &mut edge_ids, REL_CONTAINS, &fid, &cid, provenance);
            }
        }
    }

    for sym in &ir.symbols {
        let sid = symbol_id(name, &sym.qualname);
        symbol_ids.push(sid.0.clone());
        let rec = SymbolRecord {
            name: sym.name.clone(),
            qualname: sym.qualname.clone(),
            kind: sym.kind,
            visibility: sym.visibility,
            parent_symbol: sym.parent_qualname.clone(),
            file_path: file.path.clone(),
            module: ir.module.clone().unwrap_or_default(),
            crate_name: ir.crate_name.clone().unwrap_or_default(),
            loc: sym.loc,
            complexity: sym.complexity,
            start_line: sym.start_line,
            end_line: sym.end_line,
            in_test: sym.in_test,
            exact_hash: sym.exact_hash.clone(),
            structural_hash: sym.structural_hash.clone(),
        };
        put_json(&mut tx, sid.clone(), &rec, Some(provenance.clone()))?;
        add_edge(&mut tx, &mut edge_ids, REL_CONTAINS, &fid, &sid, provenance);
        add_edge(
            &mut tx,
            &mut edge_ids,
            REL_DEFINED_IN,
            &sid,
            &fid,
            provenance,
        );
        if let Some(parent) = &sym.parent_qualname {
            add_edge(
                &mut tx,
                &mut edge_ids,
                REL_PARENT_OF,
                &symbol_id(name, parent),
                &sid,
                provenance,
            );
        }
    }

    if let Some(module) = &module {
        let mid = module_id(name, module);
        let mrec = ModuleRecord {
            path: module.clone(),
            crate_name: crate_name.clone().unwrap_or_default(),
            file_path: Some(file.path.clone()),
        };
        put_json(&mut tx, mid.clone(), &mrec, Some(provenance.clone()))?;
        add_edge(&mut tx, &mut edge_ids, REL_CONTAINS, &mid, &fid, provenance);
        add_edge(
            &mut tx,
            &mut edge_ids,
            REL_IN_REPO,
            &mid,
            &repo_id(name),
            provenance,
        );
    }

    let frec = FileRecord {
        path: file.path.clone(),
        language: ir.language.clone(),
        hash: file.blob_hash.clone(),
        module: module.clone(),
        crate_name: crate_name.clone(),
        loc: ir.loc,
        parse_error: ir.parse_error.clone(),
        unwrap_count: ir.unwrap_count,
        wildcard_imports: ir.wildcard_imports,
        symbol_ids: symbol_ids.clone(),
        edge_ids: edge_ids.clone(),
        config_ids: config_ids.clone(),
    };
    put_json(&mut tx, fid.clone(), &frec, Some(provenance.clone()))?;
    add_edge(
        &mut tx,
        &mut edge_ids,
        REL_CONTAINS,
        &repo_id(name),
        &fid,
        provenance,
    );
    add_edge(
        &mut tx,
        &mut edge_ids,
        REL_AT_COMMIT,
        &fid,
        &crate::ids::commit_id(name, &head.sha),
        provenance,
    );
    add_edge(
        &mut tx,
        &mut edge_ids,
        REL_IN_REPO,
        &fid,
        &repo_id(name),
        provenance,
    );

    // rewrite file record with complete edge ids
    let mut frec = frec;
    frec.edge_ids = edge_ids;
    put_json(&mut tx, fid, &frec, Some(provenance.clone()))?;

    // index markdown files as DocRecord for content-based semantic search
    if file.path.to_lowercase().ends_with(".md") {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            let (title, tokens) = extract_md_tokens(text, &file.path);
            let kind = doc_kind(&file.path);
            let rec = DocRecord {
                path: file.path.clone(),
                repo: name.to_string(),
                title,
                kind,
                tokens,
            };
            put_json(
                &mut tx,
                doc_id(name, &file.path),
                &rec,
                Some(provenance.clone()),
            )?;
        }
    }

    if !tx.is_empty() {
        store.commit_buffered(tx, ACTOR, format!("index {}", file.path))?;
    }
    Ok(Some((ir, bytes.len())))
}

fn index_crates(
    store: &BinderStore,
    name: &str,
    ws: &WorkspaceGraph,
    provenance: &Provenance,
) -> Result<()> {
    let mut tx = Transaction::new();
    let mut any = false;
    for c in &ws.crates {
        let cid = crate_id(name, &c.name);
        let rec = CrateRecord {
            name: c.name.clone(),
            manifest_path: c.manifest_path.to_string_lossy().into_owned(),
            repository: name.to_string(),
            dependencies: c.dependencies.clone(),
        };
        put_json(&mut tx, cid.clone(), &rec, Some(provenance.clone()))?;
        add_edge_simple(&mut tx, REL_IN_REPO, &cid, &repo_id(name), provenance);
        add_edge_simple(&mut tx, REL_CONTAINS, &repo_id(name), &cid, provenance);
        for dep in &c.dependencies {
            if ws.crates.iter().any(|o| o.name == *dep) {
                add_edge_simple(
                    &mut tx,
                    REL_DEPENDS_ON,
                    &cid,
                    &crate_id(name, dep),
                    provenance,
                );
            }
        }
        any = true;
    }
    if any {
        store.commit_buffered(tx, ACTOR, "index crates")?;
    }
    Ok(())
}

fn resolve_relationships(
    store: &BinderStore,
    name: &str,
    parsed: &[(String, FileIr, String)],
    provenance: &Provenance,
) -> Result<()> {
    if parsed.is_empty() {
        return Ok(());
    }
    let symbols = load_symbol_index(store, name)?;
    let mut tx = Transaction::new();
    let mut n = 0usize;
    for (_path, ir, _) in parsed {
        let file_module = ir.module.clone().unwrap_or_default();
        for call in &ir.calls {
            if let Some(callee) = resolve_name(&symbols, &call.callee, &file_module) {
                if let Some(caller) = symbols.by_qual.get(&call.caller_qualname) {
                    add_edge_simple(&mut tx, REL_CALLS, caller, callee, provenance);
                    n += 1;
                }
            }
        }
        for r in &ir.references {
            if let Some(target) = resolve_name(&symbols, &r.target, &file_module) {
                if let Some(src) = symbols.by_qual.get(&r.source_qualname) {
                    add_edge_simple(&mut tx, REL_REFERENCES, src, target, provenance);
                    n += 1;
                }
            }
        }
        for im in &ir.impls {
            if let (Some(tr), Some(ty)) = (
                im.trait_name
                    .as_ref()
                    .and_then(|t| resolve_name(&symbols, t, &file_module)),
                resolve_name(&symbols, &im.type_name, &file_module),
            ) {
                add_edge_simple(&mut tx, REL_IMPLEMENTS, ty, tr, provenance);
                n += 1;
            }
        }
        for import in &ir.imports {
            if import.is_crate_local {
                let cleaned = import
                    .path
                    .trim_start_matches("crate::")
                    .trim_end_matches("::*")
                    .trim_end_matches("::*");
                let qual = if import.path.starts_with("crate::") {
                    format!("{}::{cleaned}", ir.crate_name.as_deref().unwrap_or(name))
                } else {
                    import.path.clone()
                };
                let mid = module_id(name, &qual);
                if store.get(&mid)?.is_some() {
                    if let Some(mod_path) = &ir.module {
                        add_edge_simple(
                            &mut tx,
                            REL_DEPENDS_ON,
                            &module_id(name, mod_path),
                            &mid,
                            provenance,
                        );
                        n += 1;
                    }
                    add_edge_simple(
                        &mut tx,
                        REL_IMPORTS,
                        &file_id(name, _path),
                        &mid,
                        provenance,
                    );
                    n += 1;
                }
            }
        }
        if n > 400 {
            store.commit_buffered(tx, ACTOR, "resolve relationships batch")?;
            tx = Transaction::new();
            n = 0;
        }
    }
    if !tx.is_empty() {
        store.commit_buffered(tx, ACTOR, "resolve relationships")?;
    }
    Ok(())
}

struct SymbolIndex {
    by_qual: HashMap<String, ObjectId>,
    by_name: HashMap<String, Vec<ObjectId>>,
}

fn load_symbol_index(store: &BinderStore, name: &str) -> Result<SymbolIndex> {
    let live = store.list(&crate::ids::prefix_sym(name))?;
    let mut by_qual = HashMap::new();
    let mut by_name: HashMap<String, Vec<ObjectId>> = HashMap::new();
    for v in live {
        let rec: SymbolRecord = decode(&v)?;
        by_qual.insert(rec.qualname.clone(), v.object_id.clone());
        by_name.entry(rec.name).or_default().push(v.object_id);
    }
    Ok(SymbolIndex { by_qual, by_name })
}

fn resolve_name<'a>(idx: &'a SymbolIndex, raw: &str, from_module: &str) -> Option<&'a ObjectId> {
    let name = raw.to_string();
    if let Some(id) = idx.by_qual.get(&name) {
        return Some(id);
    }
    let simple = name.rsplit("::").next().unwrap_or(&name);
    let local = format!("{from_module}::{simple}");
    if let Some(id) = idx.by_qual.get(&local) {
        return Some(id);
    }
    match idx.by_name.get(simple) {
        Some(ids) if ids.len() == 1 => Some(&ids[0]),
        _ => None,
    }
}

fn tombstone_file(
    store: &BinderStore,
    name: &str,
    path: &str,
    _provenance: &Provenance,
) -> Result<()> {
    let fid = file_id(name, path);
    let Some(existing) = store.get(&fid)? else {
        return Ok(());
    };
    let rec: FileRecord = match decode(&existing) {
        Ok(r) => r,
        Err(_) => {
            let mut tx = Transaction::new();
            tx.delete_object(fid);
            store.commit_buffered(tx, ACTOR, format!("drop {path}"))?;
            return Ok(());
        }
    };
    let mut tx = Transaction::new();
    for sid in &rec.symbol_ids {
        tx.delete_object(ObjectId::new(sid.clone()));
    }
    for cid in &rec.config_ids {
        tx.delete_object(ObjectId::new(cid.clone()));
    }
    for v in store.list(&crate::ids::prefix_config(name))? {
        if let Ok(cfg) = decode::<crate::model::ConfigKeyRecord>(&v) {
            if cfg.file_path == path {
                tx.delete_object(v.object_id.clone());
            }
        }
    }
    for eid in &rec.edge_ids {
        tx.delete_edge(bindb::EdgeId::new(eid.clone()));
    }
    tx.delete_object(fid);
    if !tx.is_empty() {
        store.commit_buffered(tx, ACTOR, format!("tombstone {path}"))?;
    }
    Ok(())
}

fn put_repo_and_commit(
    store: &BinderStore,
    name: &str,
    root: &Path,
    head: &GitHead,
    provenance: &Provenance,
) -> Result<()> {
    let mut tx = Transaction::new();
    put_json(
        &mut tx,
        repo_id(name),
        &RepoRecord {
            name: name.to_string(),
            root: root.display().to_string(),
            default_branch: head.branch.clone(),
        },
        Some(provenance.clone()),
    )?;
    put_json(
        &mut tx,
        crate::ids::commit_id(name, &head.sha),
        &CommitRecord {
            sha: head.sha.clone(),
            repository: name.to_string(),
            timestamp: head.timestamp.clone(),
            branch: head.branch.clone(),
            bindb_commit: None,
            crate_names: Vec::new(),
            file_paths: Vec::new(),
            symbol_qualnames: Vec::new(),
        },
        Some(provenance.clone()),
    )?;
    store.commit_buffered(tx, ACTOR, "repo/commit")?;
    Ok(())
}

fn stamp_commit_catalog(
    store: &BinderStore,
    name: &str,
    head: &GitHead,
    tree: &[TrackedFile],
    ws: &WorkspaceGraph,
    provenance: &Provenance,
) -> Result<()> {
    let bindb_commit = store.head()?.ok_or_else(|| {
        CodeintelError::Other("bindb head missing after index; cannot stamp drift catalog".into())
    })?;
    let mut crate_names: Vec<String> = ws.crates.iter().map(|c| c.name.clone()).collect();
    crate_names.sort();
    crate_names.dedup();
    let mut file_paths: Vec<String> = tree.iter().map(|f| f.path.clone()).collect();
    file_paths.sort();
    let mut symbol_qualnames: Vec<String> = Vec::new();
    for v in store.list(&crate::ids::prefix_sym(name))? {
        let rec: crate::model::SymbolRecord = decode(&v)?;
        symbol_qualnames.push(rec.qualname);
    }
    symbol_qualnames.sort();
    symbol_qualnames.dedup();
    let rec = CommitRecord {
        sha: head.sha.clone(),
        repository: name.to_string(),
        timestamp: head.timestamp.clone(),
        branch: head.branch.clone(),
        bindb_commit: Some(bindb_commit),
        crate_names,
        file_paths,
        symbol_qualnames,
    };
    let mut tx = Transaction::new();
    put_json(
        &mut tx,
        crate::ids::commit_id(name, &head.sha),
        &rec,
        Some(provenance.clone()),
    )?;
    store.commit_buffered(tx, ACTOR, format!("catalog {}", head.sha))?;
    store.flush()?;
    Ok(())
}

fn load_bookmark(store: &BinderStore, name: &str) -> Result<Option<SyncBookmark>> {
    match store.get(&sync_id(name))? {
        Some(v) => Ok(Some(decode(&v)?)),
        None => Ok(None),
    }
}

fn git_provenance(head: &GitHead) -> Provenance {
    Provenance {
        source: ProvenanceSource::Import("git".into()),
        source_ref: head.sha.clone(),
        imported_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    }
}

pub(crate) fn put_json<T: serde::Serialize>(
    tx: &mut Transaction,
    id: ObjectId,
    rec: &T,
    provenance: Option<Provenance>,
) -> Result<()> {
    let payload = serde_json::to_vec(rec)?;
    tx.put_object_with_provenance(id, payload, provenance);
    Ok(())
}

fn add_edge(
    tx: &mut Transaction,
    edge_ids: &mut Vec<String>,
    rel: &str,
    from: &ObjectId,
    to: &ObjectId,
    provenance: &Provenance,
) {
    let eid = edge_id(rel, from, to);
    edge_ids.push(eid.0.clone());
    tx.put_edge_with_provenance(
        eid,
        from.clone(),
        to.clone(),
        rel,
        None,
        Some(provenance.clone()),
    );
}

fn add_edge_simple(
    tx: &mut Transaction,
    rel: &str,
    from: &ObjectId,
    to: &ObjectId,
    provenance: &Provenance,
) {
    let eid = edge_id(rel, from, to);
    tx.put_edge_with_provenance(
        eid,
        from.clone(),
        to.clone(),
        rel,
        None,
        Some(provenance.clone()),
    );
}

fn decode<T: serde::de::DeserializeOwned>(v: &bindb::BinderObjectVersion) -> Result<T> {
    let Some(bytes) = v.payload.as_ref() else {
        return Err(CodeintelError::BadPayload(v.object_id.0.clone()));
    };
    Ok(serde_json::from_slice(bytes)?)
}

fn estimate_model_bytes(store: &BinderStore, name: &str) -> Result<u64> {
    let objs = store.list("code://")?;
    let mut n = 0u64;
    for v in objs {
        if !v.object_id.0.contains(&format!("/{name}/"))
            && !v.object_id.0.ends_with(&format!("/{name}"))
        {
            continue;
        }
        n += v.payload.as_ref().map(|p| p.len() as u64).unwrap_or(0);
    }
    Ok(n)
}

fn dir_size(path: &Path) -> Result<u64> {
    Ok(walk_size(path))
}

fn walk_size(p: &Path) -> u64 {
    let mut n = 0u64;
    let Ok(rd) = std::fs::read_dir(p) else {
        return 0;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.is_dir() {
            n += walk_size(&path);
        } else if let Ok(m) = path.metadata() {
            n += m.len();
        }
    }
    n
}

/// Open an existing store for queries.
pub fn open_store(path: &Path) -> Result<BinderStore> {
    Ok(BinderStore::open(path)?)
}

pub fn decode_obj<T: serde::de::DeserializeOwned>(v: &bindb::BinderObjectVersion) -> Result<T> {
    decode(v)
}

pub fn all_of(store: &BinderStore, prefix: &str) -> Result<Vec<bindb::BinderObjectVersion>> {
    Ok(store.list(prefix)?)
}

/// Extract title and content tokens from a markdown file for semantic search.
pub(crate) fn extract_md_tokens(text: &str, path: &str) -> (String, Vec<String>) {
    use std::collections::BTreeSet;
    let mut title = String::new();
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let heading = heading.trim_start_matches('#').trim();
            if title.is_empty() && !heading.is_empty() {
                title = heading.to_string();
            }
            tokenize_into(heading, 4, &mut tokens);
        } else {
            tokenize_into(trimmed, 4, &mut tokens);
        }
    }
    if title.is_empty() {
        title = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".md")
            .to_string();
    }
    (title, tokens.into_iter().collect())
}

fn tokenize_into(text: &str, min_len: usize, out: &mut std::collections::BTreeSet<String>) {
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
        let w = word.to_lowercase();
        let w = w.trim_matches(|c: char| c == '-' || c == '_');
        if w.len() >= min_len || crate::ids::SHORT_TECHNICAL_TERMS.contains(&w) {
            out.insert(w.to_string());
        }
    }
}

fn doc_kind(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.eq_ignore_ascii_case("CLAUDE.md") {
        "claude_md".into()
    } else if name.eq_ignore_ascii_case("README.md") {
        "readme".into()
    } else if path.contains("/specs/") || path.contains("/codev/") || path.contains("/docs/") {
        "spec".into()
    } else {
        "doc".into()
    }
}
