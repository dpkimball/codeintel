use crate::model::{SymbolKind, Visibility};
use crate::parser::{FileIr, LanguageParser, SymbolIr};

pub struct PythonParser {
    /// Module path derived from file path relative to repo root.
    pub module_path: String,
    /// Crate/package name (falls back to repo name for files without Cargo.toml).
    pub crate_name: String,
}

impl LanguageParser for PythonParser {
    fn language(&self) -> &'static str {
        "python"
    }

    fn can_parse(&self, path: &str) -> bool {
        path.ends_with(".py")
    }

    fn parse(&self, _path: &str, source: &str) -> FileIr {
        let loc = source.lines().count() as u32;
        let mut symbols = Vec::new();
        let mut current_class: Option<String> = None;
        let mut class_indent: usize = 0;

        for (lineno, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();

            // Track whether we're still inside the class body.
            if current_class.is_some() && !trimmed.is_empty() && indent <= class_indent {
                current_class = None;
            }

            if let Some(name) = extract_class(trimmed) {
                let qualname = format!("{}.{}", self.module_path, name);
                symbols.push(SymbolIr {
                    name: name.clone(),
                    qualname,
                    kind: SymbolKind::Struct,
                    visibility: if name.starts_with('_') {
                        Visibility::Private
                    } else {
                        Visibility::Public
                    },
                    parent_qualname: None,
                    loc: 1,
                    complexity: 1,
                    start_line: lineno as u32 + 1,
                    end_line: lineno as u32 + 1,
                    in_test: is_test_context(line, &self.module_path),
                    exact_hash: None,
                    structural_hash: None,
                });
                current_class = Some(name);
                class_indent = indent;
            } else if let Some(name) = extract_fn(trimmed) {
                let is_method = current_class.is_some() && indent > class_indent;
                let parent = if is_method {
                    current_class
                        .as_ref()
                        .map(|c| format!("{}.{}", self.module_path, c))
                } else {
                    None
                };
                let qualname = if let Some(ref p) = parent {
                    format!("{p}.{name}")
                } else {
                    format!("{}.{}", self.module_path, name)
                };
                symbols.push(SymbolIr {
                    name: name.clone(),
                    qualname,
                    kind: SymbolKind::Function,
                    visibility: if name.starts_with('_') {
                        Visibility::Private
                    } else {
                        Visibility::Public
                    },
                    parent_qualname: parent,
                    loc: 1,
                    complexity: 1,
                    start_line: lineno as u32 + 1,
                    end_line: lineno as u32 + 1,
                    in_test: is_test_context(line, &self.module_path),
                    exact_hash: None,
                    structural_hash: None,
                });
            }
        }

        FileIr {
            language: "python".into(),
            module: Some(self.module_path.clone()),
            crate_name: Some(self.crate_name.clone()),
            loc,
            symbols,
            ..FileIr::default()
        }
    }
}

fn extract_class(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("class ")?;
    let name = rest.split(['(', ':', ' ']).next()?;
    if is_valid_identifier(name) {
        Some(name.to_string())
    } else {
        None
    }
}

fn extract_fn(trimmed: &str) -> Option<String> {
    let rest = trimmed
        .strip_prefix("def ")
        .or_else(|| trimmed.strip_prefix("async def "))?;
    let name = rest.split(['(', ' ']).next()?;
    if is_valid_identifier(name) {
        Some(name.to_string())
    } else {
        None
    }
}

fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn is_test_context(line: &str, module_path: &str) -> bool {
    line.contains("test_") || module_path.contains("test")
}

/// Derive a Python module path from a file path relative to repo root.
/// `src/utils/__init__.py` → `src.utils`
/// `src/utils/helpers.py` → `src.utils.helpers`
pub fn python_module_path(file_path: &str) -> String {
    let path = file_path.trim_end_matches(".py");
    let path = if let Some(stripped) = path.strip_suffix("/__init__") {
        stripped
    } else {
        path
    };
    path.replace('/', ".")
}
