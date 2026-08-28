use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Module,
    Namespace,
    Type,
    Trait,
    Struct,
    Enum,
    Union,
    Function,
    Method,
    Constant,
    Static,
    Field,
    Impl,
    Use,
    Macro,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Crate,
    Restricted,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRecord {
    pub name: String,
    pub root: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRecord {
    pub sha: String,
    pub repository: String,
    pub timestamp: String,
    pub branch: String,
    /// bindb commit at which this Git SHA's graph is queryable via as_of.
    #[serde(default)]
    pub bindb_commit: Option<bindb::CommitId>,
    #[serde(default)]
    pub crate_names: Vec<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub language: String,
    pub hash: String,
    pub module: Option<String>,
    pub crate_name: Option<String>,
    pub loc: u32,
    pub parse_error: Option<String>,
    pub unwrap_count: u32,
    pub wildcard_imports: u32,
    pub symbol_ids: Vec<String>,
    pub edge_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleRecord {
    pub path: String,
    pub crate_name: String,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateRecord {
    pub name: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRecord {
    pub name: String,
    pub qualname: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub parent_symbol: Option<String>,
    pub file_path: String,
    pub module: String,
    pub crate_name: String,
    pub loc: u32,
    pub complexity: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub in_test: bool,
    pub exact_hash: Option<String>,
    pub structural_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBookmark {
    pub repository: String,
    pub sha: String,
    pub branch: String,
    pub indexed_files: u64,
    pub parsed_files: u64,
    pub skipped_unchanged: u64,
    pub symbol_count: u64,
    pub source_bytes: u64,
    pub model_bytes: u64,
    pub initial_index_ms: Option<u64>,
    pub last_incremental_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRecord {
    pub path: String,
    pub is_wildcard: bool,
    pub is_crate_local: bool,
}
