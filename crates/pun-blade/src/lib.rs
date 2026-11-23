//! Native Blade formatter: a Rust port of prettier-plugin-blade, used when `[format] blade = "native"`.

mod ast;
mod directives;
mod frontend;
mod frontmatter;
mod ignore;
mod lexer;
mod scan;
mod tree;

/// Debug dumps of the internal stages, compared against the JS plugin while porting.
#[doc(hidden)]
pub mod debug {
    use crate::directives::{Directives, TreeDirectives};
    use crate::lexer::{TokenType, collect_ignore_ranges, tokenize};

    fn tokens(text: &str) -> (String, Vec<crate::lexer::Token>) {
        let (_, content) = crate::frontmatter::parse_front_matter(text);
        let ranges = collect_ignore_ranges(&content);
        let tokens = tokenize(&content, &Directives::accept_all(), &ranges);
        (content, tokens)
    }

    pub fn dump(mode: &str, text: &str) -> String {
        let (content, tokens) = tokens(text);
        let mut out = Vec::new();
        match mode {
            "tokens" => {
                for t in &tokens {
                    out.push(format!("{:?} {} {}", t.kind, t.start, t.end));
                }
            }
            "flat" => {
                let mut directives = TreeDirectives::with_defaults();
                let names: Vec<String> = tokens
                    .iter()
                    .filter(|t| t.kind == TokenType::Directive)
                    .map(|t| {
                        let name = content[t.start..t.end].to_lowercase();
                        name.strip_prefix('@').map_or(name.clone(), str::to_owned)
                    })
                    .collect();
                directives.train(&names);
                let nodes = crate::tree::TreeBuilder::new(&tokens, &content, &directives).build();
                let n = |v: usize| if v == crate::tree::NONE { "-1".to_owned() } else { v.to_string() };
                for (i, f) in nodes.iter().enumerate() {
                    out.push(format!(
                        "{i} k{:?} p{} fc{} lc{} ns{} ts{} tc{} g{} d{}",
                        f.kind,
                        n(f.parent),
                        n(f.first_child),
                        n(f.last_child),
                        n(f.next_sibling),
                        f.token_start,
                        f.token_count,
                        f.generic_offset,
                        f.data
                    ));
                }
            }
            "ast" => {
                let mut ast = crate::ast::parse(text);
                ast.preprocess();
                dump_ast(&ast, crate::ast::ROOT, 0, &mut out);
            }
            _ => {}
        }
        out.join("\n")
    }

    fn dump_ast(ast: &crate::ast::Ast, id: usize, depth: usize, out: &mut Vec<String>) {
        let n = ast.node(id);
        let b = |v: bool| u8::from(v);
        out.push(format!(
            "{}{:?} {}-{} L{}-{} tag={:?} raw={:?} ns={:?} disp={} sc{} ws{}{} lss{} tss{} dss{} hls{} hts{} hds{} hct{} ote{} cts{} cond={:?} attrs={}",
            "  ".repeat(depth), n.kind, n.start, n.end, n.start_line, n.end_line, n.tag_name, n.raw_text, n.namespace, n.css_display,
            b(n.is_self_closing), b(n.is_whitespace_sensitive), b(n.is_indentation_sensitive),
            b(n.is_leading_space_sensitive), b(n.is_trailing_space_sensitive), b(n.is_dangling_space_sensitive),
            b(n.has_leading_spaces), b(n.has_trailing_spaces), b(n.has_dangling_spaces), b(n.has_closing_tag),
            n.open_tag_end_offset, n.closing_tag_start_offset, n.condition, n.attrs.len()
        ));
        for &a in &n.attrs {
            let an = ast.node(a);
            out.push(format!("{}@{:?} {}-{}", "  ".repeat(depth + 1), an.kind, an.start, an.end));
        }
        for &c in &n.children {
            dump_ast(ast, c, depth + 1, out);
        }
    }
}
