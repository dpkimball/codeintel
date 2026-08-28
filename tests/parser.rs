use codeintel::parser::LanguageParser;
use codeintel::rust_parser::RustParser;

#[test]
fn rust_parser_extracts_symbols_imports_and_calls() {
    let src = r#"
        use crate::inner::Helper;

        pub fn visible() {
            helper();
        }

        fn helper() {
            let _ = 1;
        }

        pub struct Thing;
        pub trait Marker {}
        impl Marker for Thing {}

        pub const N: u32 = 1;
    "#;
    let parser = RustParser {
        crate_name: "demo".into(),
        module_path: "demo".into(),
    };
    let ir = parser.parse("src/lib.rs", src);
    assert!(ir.parse_error.is_none(), "{:?}", ir.parse_error);
    assert!(ir
        .symbols
        .iter()
        .any(|s| s.name == "visible" && s.qualname == "demo::visible"));
    assert!(ir.symbols.iter().any(|s| s.name == "Thing"));
    assert!(ir.symbols.iter().any(|s| s.name == "Marker"));
    assert!(ir.imports.iter().any(|i| i.path.contains("Helper")));
    assert!(ir.calls.iter().any(|c| c.callee.contains("helper")));
    assert!(ir
        .impls
        .iter()
        .any(|i| i.trait_name.as_deref() == Some("Marker")));
}

#[test]
fn rust_parser_records_unwrap() {
    let src = r#"
        pub fn boom() {
            Some(1).unwrap();
        }
    "#;
    let parser = RustParser {
        crate_name: "demo".into(),
        module_path: "demo".into(),
    };
    let ir = parser.parse("src/lib.rs", src);
    assert!(ir.unwrap_count >= 1);
}
