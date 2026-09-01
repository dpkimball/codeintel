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
            let fm = parse_md_frontmatter(text);
            let (extracted_title, tokens) = extract_md_tokens(text, &file.path);
            let kind = doc_kind(&file.path);
            let category = if !fm.category.is_empty() {
                fm.category.clone()
            } else {
                infer_category(&file.path, &kind)
            };
            let compliance_issues =
                validate_doc_compliance(&kind, &file.path, text, &fm, &category);
            let title = if !fm.title.is_empty() {
                fm.title
            } else {
                extracted_title
            };
            let rec = DocRecord {
                path: file.path.clone(),
                repo: name.to_string(),
                title,
                kind,
                tokens,
                description: fm.description,
                category,
                use_cases: fm.use_cases,
                compliance_issues,
            };
            put_json(
                &mut tx,
                doc_id(name, &file.path),
                &rec,
                Some(provenance.clone()),
            )?;
        }
    } else if is_infra_yaml(&file.path) {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            let (title, tokens) = extract_yaml_tokens(text, &file.path);
            let kind = yaml_doc_kind(&file.path);
            let rec = DocRecord {
                path: file.path.clone(),
                repo: name.to_string(),
                title,
                kind: kind.clone(),
                tokens,
                description: String::new(),
                category: infer_category(&file.path, &kind),
                use_cases: Vec::new(),
                compliance_issues: Vec::new(),
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
        ..Default::default()
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
struct MdFrontmatter {
    title: String,
    description: String,
    category: String,
    use_cases: Vec<String>,
}

/// Parse YAML frontmatter from a `---` block at the top of a markdown file.
/// Handles `key: value` and `key: [a, b]` / block-list styles for the fields we care about.
fn parse_md_frontmatter(text: &str) -> MdFrontmatter {
    let mut fm = MdFrontmatter {
        title: String::new(),
        description: String::new(),
        category: String::new(),
        use_cases: Vec::new(),
    };
    let text = text.trim_start();
    if !text.starts_with("---") {
        return fm;
    }
    let rest = &text[3..];
    let end = match rest.find("\n---") {
        Some(i) => i,
        None => return fm,
    };
    let block = &rest[..end];

    let mut in_use_cases = false;
    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Block list item under use_cases
        if in_use_cases {
            if let Some(item) = trimmed.strip_prefix("- ") {
                fm.use_cases
                    .push(item.trim().trim_matches('"').trim_matches('\'').to_string());
                continue;
            } else if trimmed.contains(':') {
                in_use_cases = false;
                // fall through to key parsing
            } else {
                continue;
            }
        }
        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "title" => fm.title = val.trim_matches('"').trim_matches('\'').to_string(),
                "description" => {
                    fm.description = val.trim_matches('"').trim_matches('\'').to_string()
                }
                "category" => fm.category = val.trim_matches('"').trim_matches('\'').to_string(),
                "use_cases" => {
                    if val.starts_with('[') {
                        // Inline list: [item1, item2]
                        let inner = val.trim_matches(|c| c == '[' || c == ']');
                        fm.use_cases = inner
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    } else if val.is_empty() {
                        in_use_cases = true;
                    }
                }
                _ => {}
            }
        }
    }
    fm
}

const VALID_DOC_CATEGORIES: &[&str] = &[
    "deployment",
    "development",
    "architecture",
    "configuration",
    "troubleshooting",
    "testing",
    "tasks",
    "vision",
    "design",
    "specs",
    "plans",
    "reviews",
    "api",
    "setup",
    "usage",
    "examples",
    "overview",
    "doc",
];

/// Return the markdown body with the leading `---` frontmatter block stripped.
fn body_without_frontmatter(text: &str) -> &str {
    let text = text.trim_start();
    if !text.starts_with("---") {
        return text;
    }
    let rest = &text[3..];
    if let Some(end) = rest.find("\n---") {
        rest[end + 4..].trim_start_matches('\n')
    } else {
        text
    }
}

/// Path prefixes and filenames that are workflow artifacts, not canonical documentation.
/// These files are indexed for search but exempt from structural compliance checks.
const WORKFLOW_ARTIFACT_PREFIXES: &[&str] = &[
    ".github/",
    ".claude/",
    "tests/",
    "codev/",
    "backend/codev/",
    "docs/issues/",
    "docs/defects/",
    "specs/",
    "reviews/",
    "plans/",
    "resources/",
    "prompts/",
    "backend/tests/",
    "mcp-docs-server/",
    "tasks/",
];

const WORKFLOW_ARTIFACT_NAMES: &[&str] = &[
    "CONTRIBUTING.md",
    "SECURITY.md",
    "PR_CHECKLIST.md",
    "PULL_REQUEST_TEMPLATE.md",
    "AGENTS.md",
    "compliance-report.md",
];

fn is_workflow_artifact(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if WORKFLOW_ARTIFACT_NAMES.contains(&name) {
        return true;
    }
    WORKFLOW_ARTIFACT_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// Structural compliance checks run at index time against every indexed markdown document.
///
/// Returns a list of human-readable issue strings. An empty vec means the doc is compliant.
/// Checks vary by `kind`:
///   - frontmatter fields (title, description, category) required for spec/doc; skipped for
///     readme, claude_md, and YAML kinds
///   - category must be in the standard set when present
///   - H1 heading must be present
///   - heading levels must not skip (e.g. H1 → H3)
///   - fenced code blocks must name a language
fn validate_doc_compliance(
    kind: &str,
    path: &str,
    text: &str,
    fm: &MdFrontmatter,
    category: &str,
) -> Vec<String> {
    if is_workflow_artifact(path) {
        return Vec::new();
    }

    let mut issues: Vec<String> = Vec::new();

    // Frontmatter field checks (not applicable to orientation or YAML docs)
    if !matches!(
        kind,
        "readme" | "claude_md" | "ci_workflow" | "helm_template" | "helm_values"
    ) {
        if !text.trim_start().starts_with("---") {
            issues.push("frontmatter: missing --- block".into());
        } else {
            if fm.title.is_empty() {
                issues.push("frontmatter: missing title".into());
            }
            if fm.description.is_empty() {
                issues.push("frontmatter: missing description".into());
            }
            if fm.category.is_empty() {
                issues.push("frontmatter: missing category".into());
            }
        }
    }

    // Category must be in the standard set when populated
    if !category.is_empty() && !VALID_DOC_CATEGORIES.contains(&category) {
        issues.push(format!("category: '{}' not in standard set", category));
    }

    // YAML-indexed docs have no markdown structure to validate
    if matches!(kind, "ci_workflow" | "helm_template" | "helm_values") {
        return issues;
    }

    let body = body_without_frontmatter(text);

    // Walk body once: fence awareness applies to H1, hierarchy, and unlabeled fences.
    let mut in_fence = false;
    let mut has_h1 = false;
    let mut prev_level: u32 = 0;
    let mut hierarchy_issue: Option<String> = None;
    let mut unlabeled: u32 = 0;
    for line in body.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            if !in_fence {
                if rest.trim().is_empty() {
                    unlabeled += 1;
                }
                in_fence = true;
            } else {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }
        // CommonMark: ATX heading needs `#` then space (or end of line for bare `#`).
        let level = if t == "#" {
            1
        } else if let Some(rest) = t.strip_prefix('#') {
            let hashes = 1 + rest.chars().take_while(|c| *c == '#').count();
            let after = &t[hashes..];
            if after.is_empty() || after.starts_with(' ') {
                hashes as u32
            } else {
                0
            }
        } else {
            0
        };
        if level == 0 {
            continue;
        }
        if level == 1 {
            has_h1 = true;
        }
        if hierarchy_issue.is_none() && prev_level > 0 && level > prev_level + 1 {
            hierarchy_issue = Some(format!(
                "structure: heading jumps from H{} to H{}",
                prev_level, level
            ));
        }
        prev_level = level;
    }
    if !has_h1 {
        issues.push("structure: no H1 heading".into());
    }
    if let Some(msg) = hierarchy_issue {
        issues.push(msg);
    }
    if unlabeled > 0 {
        issues.push(format!(
            "code: {} block(s) without language specifier",
            unlabeled
        ));
    }

    issues
}

fn infer_category(path: &str, kind: &str) -> String {
    let lower = path.to_lowercase();
    for seg in &[
        "deployment",
        "architecture",
        "configuration",
        "troubleshooting",
        "development",
        "vision",
    ] {
        if lower.contains(&format!("/{seg}/")) || lower.contains(&format!("/{seg}.")) {
            return seg.to_string();
        }
    }
    if lower.contains("/specs/") || lower.contains("/plans/") || lower.contains("/reviews/") {
        return "specs".to_string();
    }
    match kind {
        "claude_md" => "architecture".to_string(),
        "readme" => "overview".to_string(),
        _ => "doc".to_string(),
    }
}

pub(crate) fn extract_md_tokens(text: &str, path: &str) -> (String, Vec<String>) {
    use std::collections::BTreeSet;
    let mut title = String::new();
    let mut tokens: BTreeSet<String> = BTreeSet::new();
    // Skip frontmatter block so YAML keys don't pollute tokens
    let body = if text.trim_start().starts_with("---") {
        let rest = text
            .trim_start()
            .trim_start_matches('-')
            .trim_start_matches('\n');
        if let Some(end) = rest.find("\n---") {
            rest[end + 4..].trim_start_matches('\n')
        } else {
            text
        }
    } else {
        text
    };
    for line in body.lines() {
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

fn path_contains_segment(lower: &str, segment: &str) -> bool {
    lower.contains(&format!("/{segment}/"))
        || lower.starts_with(&format!("{segment}/"))
        || lower == segment
}

/// True for YAML files that describe CI/CD or infrastructure configuration.
fn is_infra_yaml(path: &str) -> bool {
    let lower = path.to_lowercase();
    if !lower.ends_with(".yaml") && !lower.ends_with(".yml") {
        return false;
    }
    if lower.contains(".github/workflows/") || lower.contains(".github/actions/") {
        return true;
    }
    for seg in &["charts", "argo-cd", "argocd", "k8s", "kubernetes", "deploy"] {
        if path_contains_segment(&lower, seg) {
            return true;
        }
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("values") && (name.ends_with(".yaml") || name.ends_with(".yml")) {
        return true;
    }
    false
}

fn yaml_doc_kind(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.contains(".github/workflows/") || lower.contains(".github/actions/") {
        "ci_workflow".into()
    } else if lower.contains("/templates/") {
        "helm_template".into()
    } else {
        "helm_values".into()
    }
}

const YAML_SCALAR_SKIP: &[&str] = &["true", "false", "null", "yes", "no", "on", "off"];

/// Extract searchable tokens from a YAML file.
/// Tokenizes string keys and values; skips YAML scalars, anchors, and pure-numeric tokens.
fn extract_yaml_tokens(text: &str, path: &str) -> (String, Vec<String>) {
    let mut tokens = std::collections::BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Skip comments, anchors, aliases, document markers, and blank lines
        if trimmed.starts_with('#')
            || trimmed.starts_with('*')
            || trimmed.starts_with('&')
            || trimmed.starts_with("---")
            || trimmed.starts_with("...")
            || trimmed.is_empty()
        {
            continue;
        }
        // Strip list item prefix and inline comments
        let content = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let content = content.split(" #").next().unwrap_or(content).trim();
        // Tokenize then filter scalars and pure-numeric tokens word-by-word
        for word in content.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_') {
            let w = word.to_lowercase();
            let w = w.trim_matches(|c: char| c == '-' || c == '_');
            if w.is_empty() {
                continue;
            }
            if YAML_SCALAR_SKIP.contains(&w) {
                continue;
            }
            if w.parse::<f64>().is_ok() {
                continue;
            }
            if w.len() >= 4 || crate::ids::SHORT_TECHNICAL_TERMS.contains(&w) {
                tokens.insert(w.to_string());
            }
        }
    }
    // Also tokenize path components so filenames like "deploy.yaml" and directory
    // names like "workflows" or "charts" contribute to matching.
    for segment in path.split('/') {
        let seg = segment
            .trim_end_matches(".yaml")
            .trim_end_matches(".yml")
            .to_lowercase();
        let seg = seg.trim_matches(|c: char| c == '.' || c == '-' || c == '_');
        if !seg.is_empty() && (seg.len() >= 4 || crate::ids::SHORT_TECHNICAL_TERMS.contains(&seg)) {
            tokens.insert(seg.to_string());
        }
    }
    let title = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".yaml")
        .trim_end_matches(".yml")
        .to_string();
    (title, tokens.into_iter().collect())
}

#[cfg(test)]
mod yaml_tests {
    use super::*;

    #[test]
    fn is_infra_yaml_matches_workflows() {
        assert!(is_infra_yaml(".github/workflows/deploy.yaml"));
        assert!(is_infra_yaml(".github/actions/configure-cargo/action.yml"));
        assert!(is_infra_yaml("charts/binder-ai-serve/values-qa.yaml"));
        assert!(is_infra_yaml(
            "charts/binder-server/templates/deployment.yaml"
        ));
        assert!(is_infra_yaml("argo-cd/applications/binder-ai-serve.yaml"));
    }

    #[test]
    fn is_infra_yaml_rejects_non_infra() {
        assert!(!is_infra_yaml("src/main.rs"));
        assert!(!is_infra_yaml("Cargo.toml"));
        assert!(!is_infra_yaml("README.md"));
    }

    #[test]
    fn yaml_doc_kind_classifies_correctly() {
        assert_eq!(
            yaml_doc_kind(".github/workflows/deploy.yaml"),
            "ci_workflow"
        );
        assert_eq!(
            yaml_doc_kind("charts/binder-ai-serve/templates/deployment.yaml"),
            "helm_template"
        );
        assert_eq!(
            yaml_doc_kind("charts/binder-ai-serve/values-qa.yaml"),
            "helm_values"
        );
    }

    #[test]
    fn extract_yaml_tokens_surfaces_deploy_keywords() {
        let yaml = r#"
name: CI/CD (Helm)
jobs:
  build-and-push:
    runs-on: self-hosted
  update-helm-values:
    steps:
      - name: Bump image tag in Helm values
        run: yq -i '.image.tag = env(TAG)' values-qa.yaml
env:
  IMAGE_REPO: ghcr.io/dpkimball/binder-server
  SERVICE_NAME: binder-server
"#;
        let (title, tokens) = extract_yaml_tokens(yaml, ".github/workflows/deploy.yaml");
        assert_eq!(title, "deploy");
        assert!(tokens.iter().any(|t| t.contains("deploy")));
        assert!(tokens.iter().any(|t| t.contains("image")));
        assert!(tokens.iter().any(|t| t.contains("helm")));
        assert!(tokens.iter().any(|t| t.contains("binder")));
    }

    #[test]
    fn extract_yaml_tokens_skips_comments_and_booleans() {
        let yaml = r#"
# This is a comment
enabled: true
count: 42
name: keepsake-backend
"#;
        let (_, tokens) = extract_yaml_tokens(yaml, "values.yaml");
        assert!(!tokens.contains(&"true".to_string()));
        assert!(!tokens.contains(&"42".to_string()));
        assert!(tokens.iter().any(|t| t.contains("keepsake")));
    }
}

#[cfg(test)]
mod doc_compliance_tests {
    use super::*;

    fn fm(title: &str, description: &str, category: &str) -> MdFrontmatter {
        MdFrontmatter {
            title: title.into(),
            description: description.into(),
            category: category.into(),
            use_cases: Vec::new(),
        }
    }

    fn empty_fm() -> MdFrontmatter {
        fm("", "", "")
    }

    // --- body_without_frontmatter ---

    #[test]
    fn body_strips_frontmatter_block() {
        let text = "---\ntitle: Foo\n---\n# Hello\n";
        assert_eq!(body_without_frontmatter(text), "# Hello\n");
    }

    #[test]
    fn body_passthrough_when_no_frontmatter() {
        let text = "# Hello\n\nSome text.\n";
        assert_eq!(body_without_frontmatter(text), text);
    }

    #[test]
    fn body_passthrough_when_unclosed_frontmatter() {
        let text = "---\ntitle: Foo\n# Hello\n";
        // no closing ---, returns original text
        assert_eq!(body_without_frontmatter(text), text);
    }

    // --- validate_doc_compliance: frontmatter checks ---

    #[test]
    fn clean_spec_doc_produces_no_issues() {
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\nBody.\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn missing_frontmatter_block_flagged_for_spec() {
        let text = "# Title\n\nBody.\n";
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &empty_fm(), "");
        assert!(
            issues.iter().any(|i| i.contains("missing --- block")),
            "{issues:?}"
        );
    }

    #[test]
    fn missing_title_description_category_flagged() {
        let text = "---\n---\n# Title\n";
        let f = empty_fm();
        let issues = validate_doc_compliance("doc", "docs/doc.md", text, &f, "");
        assert!(
            issues.iter().any(|i| i.contains("missing title")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("missing description")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("missing category")),
            "{issues:?}"
        );
    }

    #[test]
    fn readme_skips_frontmatter_check() {
        // README.md kind never requires frontmatter
        let text = "# My Readme\n\nContent.\n";
        let issues = validate_doc_compliance("readme", "README.md", text, &empty_fm(), "overview");
        assert!(
            !issues.iter().any(|i| i.contains("frontmatter")),
            "{issues:?}"
        );
    }

    #[test]
    fn claude_md_skips_frontmatter_check() {
        let text = "# CLAUDE.md\n\n## What this is\n\nSomething.\n";
        let issues =
            validate_doc_compliance("claude_md", "CLAUDE.md", text, &empty_fm(), "architecture");
        assert!(
            !issues.iter().any(|i| i.contains("frontmatter")),
            "{issues:?}"
        );
    }

    // --- validate_doc_compliance: category checks ---

    #[test]
    fn invalid_category_flagged() {
        let text = "---\ntitle: T\ndescription: D\ncategory: banana\n---\n# Title\n";
        let f = fm("T", "D", "banana");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "banana");
        assert!(
            issues.iter().any(|i| i.contains("not in standard set")),
            "{issues:?}"
        );
    }

    #[test]
    fn valid_category_not_flagged() {
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(!issues.iter().any(|i| i.contains("category")), "{issues:?}");
    }

    // --- validate_doc_compliance: H1 checks ---

    #[test]
    fn missing_h1_flagged() {
        let text =
            "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n\n## Section\n\nBody.\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(issues.iter().any(|i| i.contains("no H1")), "{issues:?}");
    }

    #[test]
    fn h1_present_not_flagged() {
        let text =
            "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n## Section\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(!issues.iter().any(|i| i.contains("H1")), "{issues:?}");
    }

    // --- validate_doc_compliance: heading hierarchy ---

    #[test]
    fn heading_skip_h1_to_h3_flagged() {
        let text =
            "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n### Skipped\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(
            issues.iter().any(|i| i.contains("jumps from H1 to H3")),
            "{issues:?}"
        );
    }

    #[test]
    fn heading_h1_to_h2_to_h3_is_valid() {
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n## Section\n\n### Sub\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(!issues.iter().any(|i| i.contains("jumps")), "{issues:?}");
    }

    #[test]
    fn heading_going_back_up_is_valid() {
        // H3 → H2 is fine (going shallower is never a violation)
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n## A\n\n### Sub\n\n## B\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(!issues.iter().any(|i| i.contains("jumps")), "{issues:?}");
    }

    #[test]
    fn headings_inside_fenced_examples_ignored_for_hierarchy() {
        // Real docs embed sample markdown (with its own H1→H3) inside ```markdown fences.
        let text = "---\ntitle: T\ndescription: D\ncategory: architecture\n---\n\
# Real Title\n\n## Examples\n\n### Sample\n\n\
```markdown\n# TODO: Example\n\n### Nested sample heading\n```\n";
        let f = fm("T", "D", "architecture");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "architecture");
        assert!(
            !issues.iter().any(|i| i.contains("jumps")),
            "fenced example headings must not count: {issues:?}"
        );
        assert!(!issues.iter().any(|i| i.contains("no H1")), "{issues:?}");
    }

    // --- validate_doc_compliance: code blocks ---

    #[test]
    fn unlabeled_code_block_flagged() {
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n```\nsome code\n```\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(
            issues.iter().any(|i| i.contains("without language")),
            "{issues:?}"
        );
    }

    #[test]
    fn labeled_code_block_not_flagged() {
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n```bash\necho hi\n```\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(!issues.iter().any(|i| i.contains("language")), "{issues:?}");
    }

    #[test]
    fn multiple_unlabeled_blocks_counted() {
        let text = "---\ntitle: T\ndescription: D\ncategory: deployment\n---\n# Title\n\n```\na\n```\n\n```\nb\n```\n";
        let f = fm("T", "D", "deployment");
        let issues = validate_doc_compliance("spec", "docs/doc.md", text, &f, "deployment");
        assert!(
            issues.iter().any(|i| i.contains("2 block(s)")),
            "{issues:?}"
        );
    }

    // --- yaml kinds skip markdown checks ---

    #[test]
    fn ci_workflow_skips_structure_checks() {
        // YAML docs shouldn't be checked for H1 or heading hierarchy
        let issues = validate_doc_compliance(
            "ci_workflow",
            ".github/workflows/ci.yml",
            "name: CI\n",
            &empty_fm(),
            "",
        );
        assert!(!issues.iter().any(|i| i.contains("H1")), "{issues:?}");
        assert!(
            !issues.iter().any(|i| i.contains("frontmatter")),
            "{issues:?}"
        );
    }

    // --- workflow artifact exemptions ---

    #[test]
    fn github_template_exempt() {
        let issues = validate_doc_compliance(
            "doc",
            ".github/PULL_REQUEST_TEMPLATE.md",
            "## Summary\n\n- item\n",
            &empty_fm(),
            "",
        );
        assert!(issues.is_empty(), "expected exempt, got: {issues:?}");
    }

    #[test]
    fn docs_issues_path_exempt() {
        let issues = validate_doc_compliance(
            "doc",
            "docs/issues/42/validation.md",
            "# Validation\n\nSome notes.\n",
            &empty_fm(),
            "",
        );
        assert!(issues.is_empty(), "expected exempt, got: {issues:?}");
    }

    #[test]
    fn codev_path_exempt() {
        let issues =
            validate_doc_compliance("doc", "codev/specs/my-spec.md", "# Spec\n", &empty_fm(), "");
        assert!(issues.is_empty(), "expected exempt, got: {issues:?}");
    }
}
