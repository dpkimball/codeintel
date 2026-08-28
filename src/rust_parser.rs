use crate::ids::{sha256_hex, to_hex};
use crate::model::{ImportRecord, SymbolKind, Visibility};
use crate::parser::{CallIr, FileIr, ImplIr, LanguageParser, SymbolIr};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use sha2::{Digest, Sha256};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    ExprMethodCall, File, ImplItem, Item, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMod,
    ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUse, UseTree, Visibility as SynVis,
};

pub struct RustParser {
    pub crate_name: String,
    pub module_path: String,
}

impl LanguageParser for RustParser {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn can_parse(&self, path: &str) -> bool {
        path.ends_with(".rs")
    }

    fn parse(&self, _path: &str, source: &str) -> FileIr {
        match syn::parse_file(source) {
            Ok(file) => extract(&file, &self.crate_name, &self.module_path, source),
            Err(e) => FileIr {
                language: "rust".into(),
                module: Some(self.module_path.clone()),
                crate_name: Some(self.crate_name.clone()),
                loc: loc_of_source(source),
                parse_error: Some(e.to_string()),
                ..FileIr::default()
            },
        }
    }
}

fn extract(file: &File, crate_name: &str, module_path: &str, source: &str) -> FileIr {
    let mut v = Extractor {
        crate_name: crate_name.to_string(),
        module_stack: vec![module_path.to_string()],
        in_test: false,
        current_fn: None,
        ir: FileIr {
            language: "rust".into(),
            module: Some(module_path.to_string()),
            crate_name: Some(crate_name.to_string()),
            loc: loc_of_source(source),
            ..FileIr::default()
        },
    };
    v.visit_file(file);
    v.ir
}

struct Extractor {
    crate_name: String,
    module_stack: Vec<String>,
    in_test: bool,
    current_fn: Option<String>,
    ir: FileIr,
}

impl Extractor {
    fn module(&self) -> String {
        self.module_stack
            .last()
            .cloned()
            .unwrap_or_else(|| self.crate_name.clone())
    }

    fn qual(&self, name: &str) -> String {
        format!("{}::{name}", self.module())
    }

    fn push_symbol(&mut self, mut sym: SymbolIr) {
        if self.in_test {
            sym.in_test = true;
        }
        self.ir.symbols.push(sym);
    }
}

impl<'ast> Visit<'ast> for Extractor {
    fn visit_item(&mut self, item: &'ast Item) {
        let was_test = self.in_test;
        if item_is_cfg_test(item) {
            self.in_test = true;
        }
        syn::visit::visit_item(self, item);
        self.in_test = was_test;
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let name = node.sig.ident.to_string();
        let qual = self.qual(&name);
        let (start, end) = line_span(node);
        let body_tokens = node.block.to_token_stream();
        let test_fn = attrs_contain_test(&node.attrs) || self.in_test;
        self.push_symbol(SymbolIr {
            name: name.clone(),
            qualname: qual.clone(),
            kind: SymbolKind::Function,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: complexity(&body_tokens),
            start_line: start,
            end_line: end,
            in_test: test_fn,
            exact_hash: Some(sha256_hex(body_tokens.to_string().as_bytes())),
            structural_hash: Some(structural_hash(&body_tokens)),
        });
        let prev = self.current_fn.replace(qual);
        syn::visit::visit_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let name = node.ident.to_string();
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name,
            qualname: self.qual(&node.ident.to_string()),
            kind: SymbolKind::Struct,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: 1,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name: node.ident.to_string(),
            qualname: self.qual(&node.ident.to_string()),
            kind: SymbolKind::Enum,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: node.variants.len() as u32,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name: node.ident.to_string(),
            qualname: self.qual(&node.ident.to_string()),
            kind: SymbolKind::Trait,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: node.items.len() as u32,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name: node.ident.to_string(),
            qualname: self.qual(&node.ident.to_string()),
            kind: SymbolKind::Type,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: 1,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
    }

    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name: node.ident.to_string(),
            qualname: self.qual(&node.ident.to_string()),
            kind: SymbolKind::Constant,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: 1,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
    }

    fn visit_item_static(&mut self, node: &'ast ItemStatic) {
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name: node.ident.to_string(),
            qualname: self.qual(&node.ident.to_string()),
            kind: SymbolKind::Static,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: 1,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
    }

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let name = node.ident.to_string();
        let child = format!("{}::{name}", self.module());
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name,
            qualname: child.clone(),
            kind: SymbolKind::Module,
            visibility: map_vis(&node.vis),
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: 1,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
        self.module_stack.push(child);
        syn::visit::visit_item_mod(self, node);
        self.module_stack.pop();
    }

    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        collect_use(&node.tree, String::new(), &mut self.ir);
        syn::visit::visit_item_use(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let type_name = type_path(&node.self_ty);
        let trait_name = node
            .trait_
            .as_ref()
            .map(|(_, p, _)| p.to_token_stream().to_string().replace(' ', ""));
        self.ir.impls.push(ImplIr {
            type_name: type_name.clone(),
            trait_name: trait_name.clone(),
            file_module: self.module(),
        });
        let impl_qual = match &trait_name {
            Some(tr) => format!("{}::impl<{tr} for {type_name}>", self.module()),
            None => format!("{}::impl<{type_name}>", self.module()),
        };
        let (start, end) = line_span(node);
        self.push_symbol(SymbolIr {
            name: impl_qual.clone(),
            qualname: impl_qual.clone(),
            kind: SymbolKind::Impl,
            visibility: Visibility::Private,
            parent_qualname: None,
            loc: end.saturating_sub(start).saturating_add(1),
            complexity: node.items.len() as u32,
            start_line: start,
            end_line: end,
            in_test: self.in_test,
            exact_hash: None,
            structural_hash: None,
        });
        for item in &node.items {
            if let ImplItem::Fn(m) = item {
                let name = m.sig.ident.to_string();
                let qual = format!("{impl_qual}::{name}");
                let (s, e) = line_span(m);
                let body = m.block.to_token_stream();
                self.push_symbol(SymbolIr {
                    name: name.clone(),
                    qualname: qual.clone(),
                    kind: SymbolKind::Method,
                    visibility: map_vis(&m.vis),
                    parent_qualname: Some(impl_qual.clone()),
                    loc: e.saturating_sub(s).saturating_add(1),
                    complexity: complexity(&body),
                    start_line: s,
                    end_line: e,
                    in_test: self.in_test,
                    exact_hash: Some(sha256_hex(body.to_string().as_bytes())),
                    structural_hash: Some(structural_hash(&body)),
                });
                let prev = self.current_fn.replace(qual);
                syn::visit::visit_impl_item_fn(self, m);
                self.current_fn = prev;
            }
        }
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            let callee = p.path.to_token_stream().to_string().replace(' ', "");
            if let Some(caller) = &self.current_fn {
                self.ir.calls.push(CallIr {
                    caller_qualname: caller.clone(),
                    callee,
                });
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method = node.method.to_string();
        if method == "unwrap" || method == "expect" {
            self.ir.unwrap_count += 1;
        }
        if let Some(caller) = &self.current_fn {
            self.ir.calls.push(CallIr {
                caller_qualname: caller.clone(),
                callee: method,
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn collect_use(tree: &UseTree, prefix: String, ir: &mut FileIr) {
    match tree {
        UseTree::Path(p) => {
            let next = if prefix.is_empty() {
                p.ident.to_string()
            } else {
                format!("{prefix}::{}", p.ident)
            };
            collect_use(&p.tree, next, ir);
        }
        UseTree::Name(n) => {
            let path = if prefix.is_empty() {
                n.ident.to_string()
            } else {
                format!("{prefix}::{}", n.ident)
            };
            push_import(ir, path, false);
        }
        UseTree::Rename(r) => {
            let path = if prefix.is_empty() {
                r.ident.to_string()
            } else {
                format!("{prefix}::{}", r.ident)
            };
            push_import(ir, path, false);
        }
        UseTree::Glob(_) => {
            push_import(ir, format!("{prefix}::*"), true);
            ir.wildcard_imports += 1;
        }
        UseTree::Group(g) => {
            for t in &g.items {
                collect_use(t, prefix.clone(), ir);
            }
        }
    }
}

fn push_import(ir: &mut FileIr, path: String, is_wildcard: bool) {
    let is_crate_local =
        path.starts_with("crate::") || path.starts_with("super::") || path.starts_with("self::");
    ir.imports.push(ImportRecord {
        path,
        is_wildcard,
        is_crate_local,
    });
}

fn map_vis(v: &SynVis) -> Visibility {
    match v {
        SynVis::Public(_) => Visibility::Public,
        SynVis::Restricted(r) if r.path.is_ident("crate") => Visibility::Crate,
        SynVis::Restricted(_) => Visibility::Restricted,
        SynVis::Inherited => Visibility::Private,
    }
}

fn line_span<T: Spanned>(node: &T) -> (u32, u32) {
    let span = node.span();
    let start = span.start().line as u32;
    let end = span.end().line as u32;
    (start.max(1), end.max(start.max(1)))
}

fn loc_of_source(source: &str) -> u32 {
    source.lines().count() as u32
}

fn complexity(ts: &TokenStream) -> u32 {
    let mut n = 1u32;
    walk_complexity(ts, &mut n);
    n
}

fn walk_complexity(ts: &TokenStream, n: &mut u32) {
    for tt in ts.clone() {
        match tt {
            TokenTree::Ident(id) => {
                let s = id.to_string();
                if matches!(
                    s.as_str(),
                    "if" | "while" | "for" | "loop" | "match" | "and" | "or"
                ) {
                    *n += 1;
                }
            }
            TokenTree::Punct(p) => {
                if matches!(p.as_char(), '?' | '&' | '|') {
                    *n += 1;
                }
            }
            TokenTree::Group(g) => walk_complexity(&g.stream(), n),
            TokenTree::Literal(_) => {}
        }
    }
}

fn structural_hash(ts: &TokenStream) -> String {
    let mut out = String::new();
    walk_structural(ts, &mut out);
    let digest = Sha256::digest(out.as_bytes());
    to_hex(&digest)
}

fn walk_structural(ts: &TokenStream, out: &mut String) {
    for tt in ts.clone() {
        match tt {
            TokenTree::Ident(_) => out.push_str("ID"),
            TokenTree::Literal(_) => out.push_str("LIT"),
            TokenTree::Punct(p) => out.push(p.as_char()),
            TokenTree::Group(g) => {
                out.push('(');
                walk_structural(&g.stream(), out);
                out.push(')');
            }
        }
    }
}

fn type_path(ty: &syn::Type) -> String {
    ty.to_token_stream().to_string().replace(' ', "")
}

fn item_is_cfg_test(item: &Item) -> bool {
    let attrs = match item {
        Item::Fn(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        _ => return false,
    };
    attrs
        .iter()
        .any(|a| a.to_token_stream().to_string().contains("test"))
}

fn attrs_contain_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let s = a.to_token_stream().to_string();
        s.contains("test")
    })
}
