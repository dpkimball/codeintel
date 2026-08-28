use crate::model::{ImportRecord, SymbolKind, Visibility};

/// Language-agnostic file IR produced by a parser plugin.
#[derive(Debug, Clone, Default)]
pub struct FileIr {
    pub language: String,
    pub module: Option<String>,
    pub crate_name: Option<String>,
    pub loc: u32,
    pub unwrap_count: u32,
    pub wildcard_imports: u32,
    pub parse_error: Option<String>,
    pub imports: Vec<ImportRecord>,
    pub symbols: Vec<SymbolIr>,
    pub calls: Vec<CallIr>,
    pub references: Vec<RefIr>,
    pub impls: Vec<ImplIr>,
}

#[derive(Debug, Clone)]
pub struct SymbolIr {
    pub name: String,
    pub qualname: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub parent_qualname: Option<String>,
    pub loc: u32,
    pub complexity: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub in_test: bool,
    pub exact_hash: Option<String>,
    pub structural_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CallIr {
    pub caller_qualname: String,
    pub callee: String,
}

#[derive(Debug, Clone)]
pub struct RefIr {
    pub source_qualname: String,
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct ImplIr {
    pub type_name: String,
    pub trait_name: Option<String>,
    pub file_module: String,
}

pub trait LanguageParser: Send + Sync {
    fn language(&self) -> &'static str;
    fn can_parse(&self, path: &str) -> bool;
    fn parse(&self, path: &str, source: &str) -> FileIr;
}

pub fn detect_language(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".rs") {
        "rust"
    } else if lower.ends_with(".py") {
        "python"
    } else if lower.ends_with(".ts") || lower.ends_with(".tsx") {
        "typescript"
    } else if lower.ends_with(".js") || lower.ends_with(".jsx") {
        "javascript"
    } else if lower.ends_with(".go") {
        "go"
    } else {
        "unknown"
    }
}
