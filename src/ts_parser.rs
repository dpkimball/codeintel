use crate::model::{SymbolKind, Visibility};
use crate::parser::{FileIr, LanguageParser, SymbolIr};

pub struct TypeScriptParser {
    pub module_path: String,
    pub crate_name: String,
}

impl LanguageParser for TypeScriptParser {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn can_parse(&self, path: &str) -> bool {
        let p = path.to_ascii_lowercase();
        p.ends_with(".ts") || p.ends_with(".tsx")
    }

    fn parse(&self, _path: &str, source: &str) -> FileIr {
        let loc = source.lines().count() as u32;
        let mut symbols = Vec::new();

        for (lineno, line) in source.lines().enumerate() {
            let trimmed = line.trim();

            if let Some((name, kind)) = extract_ts_symbol(trimmed) {
                let qualname = format!("{}.{}", self.module_path, name);
                let in_test = line.contains(".test.") || self.module_path.contains(".test");
                symbols.push(SymbolIr {
                    name,
                    qualname,
                    kind,
                    visibility: if trimmed.starts_with("export") {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    },
                    parent_qualname: None,
                    loc: 1,
                    complexity: 1,
                    start_line: lineno as u32 + 1,
                    end_line: lineno as u32 + 1,
                    in_test,
                    exact_hash: None,
                    structural_hash: None,
                });
            }
        }

        FileIr {
            language: "typescript".into(),
            module: Some(self.module_path.clone()),
            crate_name: Some(self.crate_name.clone()),
            loc,
            symbols,
            ..FileIr::default()
        }
    }
}

/// Returns `(name, kind)` for the first recognized declaration on `trimmed`.
fn extract_ts_symbol(trimmed: &str) -> Option<(String, SymbolKind)> {
    // Strip leading export/export default/declare modifiers before matching.
    let s = strip_modifiers(trimmed);

    if let Some(name) = extract_keyword(s, "class") {
        return Some((name, SymbolKind::Struct));
    }
    if let Some(name) = extract_keyword(s, "interface") {
        return Some((name, SymbolKind::Trait));
    }
    if let Some(name) = extract_keyword(s, "type") {
        // Avoid matching `typeof`, `type=`, etc.
        if s.starts_with("type ") {
            return Some((name, SymbolKind::Type));
        }
    }
    if let Some(name) = extract_keyword(s, "function") {
        return Some((name, SymbolKind::Function));
    }
    if let Some(name) = extract_keyword(s, "async function") {
        return Some((name, SymbolKind::Function));
    }
    // `const Foo = ` or `const Foo: Type = ` (arrow functions, React components, etc.)
    if let Some(name) = extract_const_name(s) {
        return Some((name, SymbolKind::Function));
    }

    None
}

fn strip_modifiers(s: &str) -> &str {
    let s = s.strip_prefix("export default ").unwrap_or(s);
    let s = s.strip_prefix("export ").unwrap_or(s);
    let s = s.strip_prefix("declare ").unwrap_or(s);
    s
}

fn extract_keyword(s: &str, keyword: &str) -> Option<String> {
    let rest = s.strip_prefix(keyword)?.strip_prefix(' ')?;
    let name = rest.split(['<', '(', ' ', '{', ':']).next()?;
    if is_valid_ts_identifier(name) {
        Some(name.to_string())
    } else {
        None
    }
}

/// `const Foo = ` or `const Foo: SomeType = `
fn extract_const_name(s: &str) -> Option<String> {
    let rest = s.strip_prefix("const ")?.split_once(" = ")?.0;
    // rest might be `Foo` or `Foo: SomeType`
    let name = rest.split(':').next()?.trim();
    if is_valid_ts_identifier(name) && name.chars().next()?.is_uppercase() {
        // Only emit PascalCase consts — arrow-function components and named objects.
        Some(name.to_string())
    } else {
        None
    }
}

fn is_valid_ts_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Derive a TypeScript module path from a file path (relative to repo root).
/// `src/components/Button.tsx` → `src.components.Button`
/// `.d.ts` is stripped as a unit first to avoid leaving a trailing `.d`.
pub fn ts_module_path(file_path: &str) -> String {
    let path = file_path
        .strip_suffix(".d.ts")
        .or_else(|| file_path.strip_suffix(".tsx"))
        .or_else(|| file_path.strip_suffix(".ts"))
        .or_else(|| file_path.strip_suffix(".jsx"))
        .or_else(|| file_path.strip_suffix(".js"))
        .unwrap_or(file_path);
    path.replace(['/', '\\'], ".")
}
