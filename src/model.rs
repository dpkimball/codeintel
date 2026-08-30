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
    #[serde(default)]
    pub symbol_qualnames: Vec<String>,
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
    #[serde(default)]
    pub config_ids: Vec<String>,
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
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
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

/// Flattened key from `dpk.toml` (and later other manifests). Values are tokens, not blobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigKeyRecord {
    pub file_path: String,
    pub key: String,
    #[serde(default)]
    pub value_tokens: Vec<String>,
}

/// GitHub issue fetched via `gh issue list`. Stored per codeintel repo name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueRecord {
    pub number: u64,
    pub title: String,
    /// Body truncated to 2000 chars at index time.
    pub body: String,
    pub url: String,
    pub state: String,
    pub labels: Vec<String>,
    /// Full GitHub repo name, e.g. "dpkimball/binder-ai".
    pub github_repo: String,
    /// codeintel repo name, e.g. "binder-ai".
    pub repo_name: String,
    /// Words (≥ 4 chars) extracted from title + body for semantic matching.
    pub tokens: Vec<String>,
}

/// Markdown document indexed for content-based semantic search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocRecord {
    pub path: String,
    pub repo: String,
    /// First `# Heading` or the filename stem.
    pub title: String,
    /// "spec", "claude_md", "readme", or "doc".
    pub kind: String,
    /// Words (≥ 4 chars) extracted from headings and body text.
    pub tokens: Vec<String>,
}
