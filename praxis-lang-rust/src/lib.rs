use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};


pub struct RustAnalyzer;

impl RustAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageAnalyzer for RustAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let Ok(syntax) = syn::parse_file(&file.content) else {
            return Vec::new();
        };

        let mut symbols = Vec::new();
        extract_items(&syntax.items, file, &mut symbols);
        symbols
    }

    fn extract_dependencies(&self, repo_root: &Path) -> Vec<Dependency> {
        let cargo_path = repo_root.join("Cargo.toml");
        let Ok(content) = std::fs::read_to_string(&cargo_path) else {
            return Vec::new();
        };
        let Ok(doc) = content.parse::<toml::Table>() else {
            return Vec::new();
        };

        let mut deps = Vec::new();

        let Some(dep_table) = doc.get("dependencies").and_then(|v| v.as_table()) else {
            return deps;
        };

        for (name, value) in dep_table {
            let (version, features) = parse_dep_value(value);
            deps.push(Dependency {
                name: name.clone(),
                version,
                features,
            });
        }

        deps.sort_by(|a, b| a.name.cmp(&b.name));
        deps
    }

    fn summarize_file(&self, file: &FileEntry) -> Option<String> {
        let mut summary = String::new();

        for line in file.content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//!") {
                let content = trimmed.strip_prefix("//!").unwrap_or("").trim();
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(content);
            } else if trimmed.starts_with("///") && summary.is_empty() {
                let content = trimmed.strip_prefix("///").unwrap_or("").trim();
                summary.push_str(content);
            } else if !summary.is_empty() && !trimmed.starts_with("//!") {
                break;
            }
        }

        if summary.is_empty() {
            return None;
        }

        if summary.len() > 300 {
            let mut end = 300;
            while end > 0 && !summary.is_char_boundary(end) {
                end -= 1;
            }
            summary.truncate(end);
            summary.push_str("...");
        }

        Some(summary)
    }
}

fn extract_items(items: &[syn::Item], file: &FileEntry, symbols: &mut Vec<Symbol>) {
    for item in items {
        match item {
            syn::Item::Fn(f) => {
                let vis = map_visibility(&f.vis);
                let sig = format_fn_signature(&f.sig, &vis);
                let (start, end) = span_lines(&f.sig.fn_token.span, &file.content);
                symbols.push(Symbol {
                    name: f.sig.ident.to_string(),
                    kind: SymbolKind::Function,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig,
                });
            }
            syn::Item::Struct(s) => {
                let vis = map_visibility(&s.vis);
                let sig = format!("{} struct {}", vis, s.ident);
                let (start, end) = span_lines(&s.struct_token.span, &file.content);
                symbols.push(Symbol {
                    name: s.ident.to_string(),
                    kind: SymbolKind::Struct,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig.trim().to_string(),
                });
            }
            syn::Item::Enum(e) => {
                let vis = map_visibility(&e.vis);
                let sig = format!("{} enum {}", vis, e.ident);
                let (start, end) = span_lines(&e.enum_token.span, &file.content);
                symbols.push(Symbol {
                    name: e.ident.to_string(),
                    kind: SymbolKind::Enum,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig.trim().to_string(),
                });
            }
            syn::Item::Trait(t) => {
                let vis = map_visibility(&t.vis);
                let sig = format!("{} trait {}", vis, t.ident);
                let (start, end) = span_lines(&t.trait_token.span, &file.content);
                symbols.push(Symbol {
                    name: t.ident.to_string(),
                    kind: SymbolKind::Trait,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig.trim().to_string(),
                });
            }
            syn::Item::Impl(imp) => {
                for impl_item in &imp.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        let vis = map_visibility(&method.vis);
                        let self_ty = quote_type(&imp.self_ty);
                        let sig = format_method_signature(&method.sig, &vis, &self_ty);
                        let (start, end) =
                            span_lines(&method.sig.fn_token.span, &file.content);
                        symbols.push(Symbol {
                            name: method.sig.ident.to_string(),
                            kind: SymbolKind::Method,
                            file: file.path.clone(),
                            visibility: Some(vis),
                            start_line: start,
                            end_line: end,
                            signature: sig,
                        });
                    }
                }
            }
            syn::Item::Mod(m) => {
                let vis = map_visibility(&m.vis);
                let sig = format!("{} mod {}", vis, m.ident);
                let (start, end) = span_lines(&m.mod_token.span, &file.content);
                symbols.push(Symbol {
                    name: m.ident.to_string(),
                    kind: SymbolKind::Module,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig.trim().to_string(),
                });

                if let Some((_, items)) = &m.content {
                    extract_items(items, file, symbols);
                }
            }
            syn::Item::Const(c) => {
                let vis = map_visibility(&c.vis);
                let ty = quote_type(&c.ty);
                let sig = format!("{} const {}: {}", vis, c.ident, ty);
                let (start, end) = span_lines(&c.const_token.span, &file.content);
                symbols.push(Symbol {
                    name: c.ident.to_string(),
                    kind: SymbolKind::Constant,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig.trim().to_string(),
                });
            }
            syn::Item::Static(s) => {
                let vis = map_visibility(&s.vis);
                let ty = quote_type(&s.ty);
                let sig = format!("{} static {}: {}", vis, s.ident, ty);
                let (start, end) = span_lines(&s.static_token.span, &file.content);
                symbols.push(Symbol {
                    name: s.ident.to_string(),
                    kind: SymbolKind::Constant,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: start,
                    end_line: end,
                    signature: sig.trim().to_string(),
                });
            }
            _ => {}
        }
    }
}

fn map_visibility(vis: &syn::Visibility) -> Visibility {
    match vis {
        syn::Visibility::Public(_) => Visibility::Public,
        syn::Visibility::Restricted(r) => {
            let path = r.path.segments.last().map(|s| s.ident.to_string());
            match path.as_deref() {
                Some("crate") => Visibility::Crate,
                _ => Visibility::Private,
            }
        }
        syn::Visibility::Inherited => Visibility::Private,
    }
}

fn format_fn_signature(sig: &syn::Signature, vis: &Visibility) -> String {
    let vis_str = match vis {
        Visibility::Public => "pub ",
        Visibility::Crate => "pub(crate) ",
        Visibility::Private => "",
    };
    let asyncness = if sig.asyncness.is_some() { "async " } else { "" };
    let unsafety = if sig.unsafety.is_some() { "unsafe " } else { "" };
    let name = &sig.ident;

    let mut params = Vec::new();
    for arg in &sig.inputs {
        params.push(quote::quote!(#arg).to_string());
    }
    let params = params.join(", ");

    let ret = match &sig.output {
        syn::ReturnType::Default => String::new(),
        syn::ReturnType::Type(_, ty) => format!(" -> {}", quote::quote!(#ty)),
    };

    format!("{vis_str}{unsafety}{asyncness}fn {name}({params}){ret}")
}

fn format_method_signature(sig: &syn::Signature, vis: &Visibility, self_ty: &str) -> String {
    let base = format_fn_signature(sig, vis);
    format!("impl {self_ty} :: {base}")
}

fn quote_type(ty: &syn::Type) -> String {
    quote::quote!(#ty).to_string()
}

fn span_lines(span: &proc_macro2::Span, _content: &str) -> (usize, usize) {
    let start = span.start().line;
    let end = span.end().line;
    (start, end)
}

fn parse_dep_value(value: &toml::Value) -> (Option<String>, Vec<String>) {
    match value {
        toml::Value::String(v) => (Some(v.clone()), Vec::new()),
        toml::Value::Table(t) => {
            let version = t.get("version").and_then(|v| v.as_str()).map(String::from);
            let features = t
                .get("features")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let mut feats = Vec::new();
                    for f in arr {
                        if let Some(s) = f.as_str() {
                            feats.push(s.to_string());
                        }
                    }
                    feats
                })
                .unwrap_or_default();
            (version, features)
        }
        _ => (None, Vec::new()),
    }
}
