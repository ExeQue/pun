use std::collections::HashMap;

use crate::problem::Span;
use mago_syntax::cst::*;

use crate::rules::support::statement_edits::tokens;
use crate::syntax::node_span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Class,
    Function,
    Constant,
}

#[derive(Clone, Debug)]
pub(crate) struct PhpName {
    pub span: Span,
    pub written: String,
    pub kind: Kind,
    pub resolved: Option<String>,
    pub new: bool,
    pub static_call: bool,
    pub extends: bool,
    pub implements: bool,
    pub class_const_fetch: Option<String>,
    pub namespace: Option<String>,
    pub class: Option<String>,
}

impl PhpName {
    pub(crate) fn original(&self) -> &str {
        let written = self.written.trim_start_matches('\\');
        match written.get(..10) {
            Some(prefix) if written.len() > 10 && prefix.eq_ignore_ascii_case("namespace\\") => {
                &written[10..]
            }
            _ => written,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct UseItemView {
    pub name: String,
    pub name_span: Span,
    pub alias: Option<String>,
    pub kind: Kind,
    pub span: Span,
}

impl UseItemView {
    pub(crate) fn alias_or_last(&self) -> &str {
        match &self.alias {
            Some(alias) => alias,
            None => self.name.rsplit('\\').next().unwrap_or_default(),
        }
    }

    pub(crate) fn last(&self) -> &str {
        self.name.rsplit('\\').next().unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct UseView {
    pub span: Span,
    pub kind: Option<Kind>,
    pub prefix: Option<String>,
    pub items: Vec<UseItemView>,
}

impl UseView {
    pub(crate) fn prefix(&self) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}\\"),
            None => String::new(),
        }
    }

    pub(crate) fn full_name(&self, i: usize) -> String {
        format!("{}{}", self.prefix(), self.items[i].name)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Root {
    pub name: Option<String>,
    pub is_namespace: bool,
    pub uses: Vec<UseView>,
    pub inner: Span,
    pub stmts: Vec<StmtView>,
    pub header: Option<Header>,
}

#[derive(Clone, Debug)]
pub(crate) struct Header {
    pub span: Span,
    pub list_pos: usize,
    pub braced: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StmtKind {
    Use(usize),
    Declare,
    Namespace,
    Nop,
    Inline,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StmtView {
    pub span: Span,
    pub comment_start: usize,
    pub kind: StmtKind,
}

#[derive(Clone, Debug)]
pub(crate) struct ClassLikeView {
    pub name: String,
    pub short: String,
    pub is_class: bool,
    pub implements: Vec<(Span, String)>,
    pub implements_span: Option<Span>,
    pub root: usize,
    pub start: usize,
}

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Owners {
    pub stmt: bool,
    pub param: bool,
    pub renamed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DocView {
    pub span: Span,
    pub owners: Owners,
    pub node_start: usize,
    pub is_last: bool,
    pub namespace: Option<String>,
    pub class: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FileView {
    pub names: Vec<PhpName>,
    pub roots: Vec<Root>,
    pub class_likes: Vec<ClassLikeView>,
    pub docs: Vec<DocView>,
    pub namespaces: usize,
    pub inline_html: bool,
}

impl FileView {
    pub(crate) fn uses_root(&self) -> &Root {
        match self.namespaces {
            1 => self.roots.iter().find(|r| r.is_namespace).unwrap(),
            _ => &self.roots[0],
        }
    }

    pub(crate) fn placement_root(&self) -> usize {
        self.roots.iter().position(|r| r.is_namespace).unwrap_or(0)
    }

    pub(crate) fn of(source: &str, program: &Program<'_>) -> FileView {
        let mut walk = Walk {
            view: FileView::default(),
            namespace: None,
            aliases: Aliases::default(),
            classes: Vec::new(),
            comments: comment_runs(source),
            root: 0,
        };
        walk.view.roots.push(Root {
            name: None,
            is_namespace: false,
            uses: Vec::new(),
            inner: Span::new(0, source.len()),
            stmts: Vec::new(),
            header: None,
        });
        walk.view.namespaces = program
            .statements
            .iter()
            .filter(|s| matches!(s, Statement::Namespace(_)))
            .count();
        walk.node(Node::Program(program));
        walk.view.names.sort_by_key(|n| n.span.start);
        let mut view = walk.view;
        let all = crate::rules::support::statements::lex(source);
        view.roots[0].stmts =
            stmt_list(source, &all, program.statements.iter(), view.roots[0].inner);
        let mut root = 0;
        for statement in &program.statements {
            let Statement::Namespace(namespace) = statement else {
                continue;
            };
            root += 1;
            let body: Vec<&Statement<'_>> = match &namespace.body {
                NamespaceBody::Implicit(body) => body.statements.iter().collect(),
                NamespaceBody::BraceDelimited(block) => block.statements.iter().collect(),
            };
            let inner = view.roots[root].inner;
            let inner = match &namespace.body {
                NamespaceBody::Implicit(_) => Span::new(
                    inner.start,
                    all.iter()
                        .find(|t| t.start >= inner.end && !t.kind.is_trivia())
                        .map_or(source.len(), |t| t.start),
                ),
                NamespaceBody::BraceDelimited(_) => inner,
            };
            view.roots[root].stmts = stmt_list(source, &all, body.into_iter(), inner);
        }
        view
    }
}

#[derive(Default)]
struct Aliases {
    class: HashMap<String, String>,
    function: HashMap<String, String>,
    constant: HashMap<String, String>,
}

struct Walk {
    view: FileView,
    namespace: Option<String>,
    aliases: Aliases,
    classes: Vec<String>,
    comments: HashMap<usize, (usize, Vec<Span>)>,
    root: usize,
}

fn comment_runs(source: &str) -> HashMap<usize, (usize, Vec<Span>)> {
    let mut runs = HashMap::new();
    let mut docs: Vec<Span> = Vec::new();
    let mut last_end = 0;
    let mut has_comment = false;
    for token in tokens(source, 0, source.len()) {
        if token.kind.is_comment() {
            has_comment = true;
            let text = &source[token.start..token.end];
            if text.starts_with("/**")
                && text
                    .as_bytes()
                    .get(3)
                    .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                docs.push(Span::new(token.start, token.end));
            }
            continue;
        }
        if token.kind.is_trivia() {
            continue;
        }
        if has_comment {
            runs.insert(token.start, (last_end, std::mem::take(&mut docs)));
        }
        has_comment = false;
        docs.clear();
        last_end = token.end;
    }
    runs
}

fn text(id: &Identifier<'_>) -> String {
    String::from_utf8_lossy(id.value()).into_owned()
}

impl Walk {
    fn resolve(&self, written: &str, kind: Kind) -> Option<String> {
        if let Some(fq) = written.strip_prefix('\\') {
            return Some(fq.to_string());
        }
        let concat = |ns: &Option<String>, rest: &str| match ns {
            Some(ns) => format!("{ns}\\{rest}"),
            None => rest.to_string(),
        };
        if written.len() > 10 && written[..10].eq_ignore_ascii_case("namespace\\") {
            return Some(concat(&self.namespace, &written[10..]));
        }
        match written.split_once('\\') {
            Some((first, rest)) => {
                if let Some(alias) = self.aliases.class.get(&first.to_ascii_lowercase()) {
                    return Some(format!("{alias}\\{rest}"));
                }
            }
            None => {
                let found = match kind {
                    Kind::Class => self.aliases.class.get(&written.to_ascii_lowercase()),
                    Kind::Function => self.aliases.function.get(&written.to_ascii_lowercase()),
                    Kind::Constant => self.aliases.constant.get(written),
                };
                if let Some(alias) = found {
                    return Some(alias.clone());
                }
                if kind != Kind::Class {
                    return match self.namespace {
                        None => Some(written.to_string()),
                        Some(_) => None,
                    };
                }
            }
        }
        Some(concat(&self.namespace, written))
    }

    fn name(&mut self, id: &Identifier<'_>, kind: Kind) -> &mut PhpName {
        let written = text(id);
        let resolved = self.resolve(&written, kind);
        self.view.names.push(PhpName {
            span: node_span(id),
            written,
            kind,
            resolved,
            new: false,
            static_call: false,
            extends: false,
            implements: false,
            class_const_fetch: None,
            namespace: self.namespace.clone(),
            class: self.classes.last().cloned(),
        });
        self.view.names.last_mut().unwrap()
    }

    fn class_expression(&mut self, expr: &Expression<'_>) -> Option<&mut PhpName> {
        match expr {
            Expression::Identifier(id) => Some(self.name(id, Kind::Class)),
            _ => None,
        }
    }

    fn uses(&mut self, r#use: &Use<'_>) {
        let item = |name: &Identifier<'_>, alias: &Option<UseItemAlias<'_>>, kind: Kind, span| {
            UseItemView {
                name: text(name).trim_start_matches('\\').to_string(),
                name_span: node_span(name),
                alias: alias
                    .as_ref()
                    .map(|a| String::from_utf8_lossy(a.identifier.value).into_owned()),
                kind,
                span,
            }
        };
        let kind_of = |t: &UseType<'_>| match t {
            UseType::Function(_) => Kind::Function,
            UseType::Const(_) => Kind::Constant,
        };
        let view = match &r#use.items {
            UseItems::Sequence(s) => UseView {
                span: node_span(r#use),
                kind: Some(Kind::Class),
                prefix: None,
                items: s
                    .items
                    .iter()
                    .map(|i| item(&i.name, &i.alias, Kind::Class, node_span(i)))
                    .collect(),
            },
            UseItems::TypedSequence(s) => UseView {
                span: node_span(r#use),
                kind: Some(kind_of(&s.r#type)),
                prefix: None,
                items: s
                    .items
                    .iter()
                    .map(|i| item(&i.name, &i.alias, kind_of(&s.r#type), node_span(i)))
                    .collect(),
            },
            UseItems::TypedList(l) => UseView {
                span: node_span(r#use),
                kind: Some(kind_of(&l.r#type)),
                prefix: Some(text(&l.namespace).trim_start_matches('\\').to_string()),
                items: l
                    .items
                    .iter()
                    .map(|i| item(&i.name, &i.alias, kind_of(&l.r#type), node_span(i)))
                    .collect(),
            },
            UseItems::MixedList(l) => UseView {
                span: node_span(r#use),
                kind: None,
                prefix: Some(text(&l.namespace).trim_start_matches('\\').to_string()),
                items: l
                    .items
                    .iter()
                    .map(|i| {
                        let kind = i.r#type.as_ref().map_or(Kind::Class, kind_of);
                        item(&i.item.name, &i.item.alias, kind, node_span(i))
                    })
                    .collect(),
            },
        };
        for i in 0..view.items.len() {
            let name = view.full_name(i);
            let item = &view.items[i];
            let alias = item.alias_or_last().to_string();
            match item.kind {
                Kind::Class => self.aliases.class.insert(alias.to_ascii_lowercase(), name),
                Kind::Function => self
                    .aliases
                    .function
                    .insert(alias.to_ascii_lowercase(), name),
                Kind::Constant => self.aliases.constant.insert(alias, name),
            };
        }
        self.view.roots[self.root].uses.push(view);
    }

    fn start_namespace(&mut self, namespace: &Namespace<'_>) {
        self.namespace = namespace.name.as_ref().map(|n| text(n));
        self.aliases = Aliases::default();
        let inner = match &namespace.body {
            NamespaceBody::Implicit(body) => Span::new(
                node_span(&body.terminator).end,
                body.statements
                    .last()
                    .map_or(node_span(&body.terminator).end, |s| node_span(s).end),
            ),
            NamespaceBody::BraceDelimited(block) => Span::new(
                block.left_brace.end.offset as usize,
                block.right_brace.start.offset as usize,
            ),
        };
        let start = node_span(namespace).start;
        let list_pos = match &namespace.name {
            Some(name) => node_span(name).end,
            None => start + "namespace".len(),
        };
        self.view.roots.push(Root {
            name: self.namespace.clone(),
            is_namespace: true,
            uses: Vec::new(),
            inner,
            stmts: Vec::new(),
            header: Some(Header {
                span: node_span(namespace),
                list_pos,
                braced: matches!(namespace.body, NamespaceBody::BraceDelimited(_)),
            }),
        });
        self.root = self.view.roots.len() - 1;
    }

    fn class_like(&mut self, start: usize, short: &LocalIdentifier<'_>, is_class: bool) -> String {
        let short = String::from_utf8_lossy(short.value).into_owned();
        let name = match &self.namespace {
            Some(ns) => format!("{ns}\\{short}"),
            None => short.clone(),
        };
        self.view.class_likes.push(ClassLikeView {
            name: name.clone(),
            short,
            is_class,
            implements: Vec::new(),
            implements_span: None,
            root: self.root,
            start,
        });
        name
    }

    fn implements(&mut self, implements: &Implements<'_>, of_class: bool) {
        let mut list = Vec::new();
        for id in &implements.types {
            list.push((node_span(id), text(id)));
            self.name(id, Kind::Class).implements = true;
        }
        if of_class {
            let class = self.view.class_likes.last_mut().unwrap();
            class.implements_span = Some(Span::new(list[0].0.start, list.last().unwrap().0.end));
            class.implements = list;
        }
    }

    fn doc(&mut self, node: &Node<'_, '_>) {
        let start = node_span(node).start;
        let Some((_, docs)) = self.comments.get(&start).cloned() else {
            return;
        };
        let stmt = matches!(node, Node::Statement(_) | Node::ClassLikeMember(_));
        let param = matches!(node, Node::FunctionLikeParameter(_));
        let renamed = matches!(
            node,
            Node::Statement(Statement::Expression(_) | Statement::If(_))
                | Node::Property(_)
                | Node::Function(_)
                | Node::Method(_)
                | Node::Closure(_)
                | Node::ArrowFunction(_)
                | Node::Class(_)
                | Node::Interface(_)
                | Node::Trait(_)
                | Node::Enum(_)
        );
        let count = docs.len();
        for (i, span) in docs.into_iter().enumerate() {
            match self.view.docs.iter_mut().find(|d| d.span == span) {
                Some(doc) => {
                    doc.owners.stmt |= stmt;
                    doc.owners.param |= param;
                    doc.owners.renamed |= renamed;
                }
                None => self.view.docs.push(DocView {
                    span,
                    owners: Owners {
                        stmt,
                        param,
                        renamed,
                    },
                    node_start: start,
                    is_last: i + 1 == count,
                    namespace: self.namespace.clone(),
                    class: self.classes.last().cloned(),
                }),
            }
        }
    }

    fn node(&mut self, node: Node<'_, '_>) {
        let mut pushed_class = false;
        match node {
            Node::Namespace(namespace) => self.start_namespace(namespace),
            Node::Use(r#use) => {
                self.uses(r#use);
                return;
            }
            Node::Class(class) => {
                let name = self.class_like(node_span(class).start, &class.name, true);
                self.classes.push(name);
                pushed_class = true;
                if let Some(extends) = &class.extends {
                    for id in &extends.types {
                        self.name(id, Kind::Class).extends = true;
                    }
                }
                if let Some(implements) = &class.implements {
                    self.implements(implements, true);
                }
            }
            Node::AnonymousClass(class) => {
                self.classes.push(String::new());
                pushed_class = true;
                if let Some(extends) = &class.extends {
                    for id in &extends.types {
                        self.name(id, Kind::Class).extends = true;
                    }
                }
                if let Some(implements) = &class.implements {
                    for id in &implements.types {
                        self.name(id, Kind::Class).implements = true;
                    }
                }
            }
            Node::Interface(interface) => {
                let name = self.class_like(node_span(interface).start, &interface.name, false);
                self.classes.push(name);
                pushed_class = true;
                if let Some(extends) = &interface.extends {
                    for id in &extends.types {
                        self.name(id, Kind::Class);
                    }
                }
            }
            Node::Trait(r#trait) => {
                let name = self.class_like(node_span(r#trait).start, &r#trait.name, false);
                self.classes.push(name);
                pushed_class = true;
            }
            Node::Enum(r#enum) => {
                let name = self.class_like(node_span(r#enum).start, &r#enum.name, false);
                self.classes.push(name);
                pushed_class = true;
                if let Some(implements) = &r#enum.implements {
                    self.implements(implements, false);
                }
            }
            Node::Inline(inline) if is_inline_html(inline) => self.view.inline_html = true,
            Node::Attribute(attribute) => {
                self.name(&attribute.name, Kind::Class);
            }
            Node::Hint(Hint::Identifier(id)) => {
                self.name(id, Kind::Class);
            }
            Node::TraitUse(r#use) => {
                for id in &r#use.trait_names {
                    self.name(id, Kind::Class);
                }
            }
            Node::TraitUseAbsoluteMethodReference(reference) => {
                self.name(&reference.trait_name, Kind::Class);
            }
            Node::TraitUseAdaptation(mago_syntax::cst::TraitUseAdaptation::Precedence(
                adaptation,
            )) => {
                for id in &adaptation.trait_names {
                    self.name(id, Kind::Class);
                }
            }
            Node::Instantiation(new) => {
                if let Some(name) = self.class_expression(new.class) {
                    name.new = true;
                }
            }
            Node::StaticMethodCall(call) => {
                if let Some(name) = self.class_expression(call.class) {
                    name.static_call = true;
                }
            }
            Node::StaticMethodPartialApplication(call) => {
                if let Some(name) = self.class_expression(call.class) {
                    name.static_call = true;
                }
            }
            Node::StaticPropertyAccess(access) => {
                self.class_expression(access.class);
            }
            Node::ClassConstantAccess(access) => {
                let constant = match &access.constant {
                    ClassLikeConstantSelector::Identifier(id) => {
                        Some(String::from_utf8_lossy(id.value).into_owned())
                    }
                    _ => None,
                };
                if let Some(name) = self.class_expression(access.class) {
                    name.class_const_fetch = constant;
                }
            }
            Node::Binary(binary) if binary.operator.is_instanceof() => {
                self.class_expression(binary.rhs);
            }
            Node::FunctionCall(call) => {
                if let Expression::Identifier(id) = call.function {
                    self.name(id, Kind::Function);
                }
            }
            Node::FunctionPartialApplication(call) => {
                if let Expression::Identifier(id) = call.function {
                    self.name(id, Kind::Function);
                }
            }
            Node::ConstantAccess(access) => {
                self.name(&access.name, Kind::Constant);
            }
            _ => {}
        }
        self.doc(&node);
        for child in node.children() {
            self.node(child);
        }
        if pushed_class {
            self.classes.pop();
        }
    }
}

fn stmt_list<'a, 'b: 'a>(
    source: &str,
    all: &[crate::rules::support::statements::Token],
    statements: impl Iterator<Item = &'a Statement<'b>>,
    inner: Span,
) -> Vec<StmtView> {
    let mut list = Vec::new();
    let mut uses = 0;
    for statement in statements {
        let kind = match statement {
            Statement::OpeningTag(_) | Statement::ClosingTag(_) | Statement::Noop(_) => continue,
            Statement::Inline(inline) if !is_inline_html(inline) => continue,
            Statement::Use(_) => {
                uses += 1;
                StmtKind::Use(uses - 1)
            }
            Statement::Declare(_) => StmtKind::Declare,
            Statement::Namespace(_) => StmtKind::Namespace,
            Statement::Inline(_) => StmtKind::Inline,
            _ => StmtKind::Other,
        };
        let span = node_span(statement);
        let comment_start = match statement {
            Statement::Inline(_) => span.start,
            _ => {
                let before =
                    crate::rules::support::statements::previous_token_end(source, all, span.start);
                all.iter()
                    .find(|t| t.start >= before && t.end <= span.start && t.kind.is_comment())
                    .map_or(span.start, |t| t.start)
            }
        };
        list.push(StmtView {
            span,
            comment_start,
            kind,
        });
    }
    let after = list.last().map_or(inner.start, |s| s.span.end);
    if crate::rules::support::statements::trailing_comment(all, after, inner.end) {
        let comments: Vec<_> = all
            .iter()
            .filter(|t| t.start >= after && t.end <= inner.end && t.kind.is_comment())
            .collect();
        let end = comments.last().unwrap().end;
        list.push(StmtView {
            span: Span::new(end, end),
            comment_start: comments[0].start,
            kind: StmtKind::Nop,
        });
    }
    list
}

fn is_inline_html(inline: &Inline<'_>) -> bool {
    !(matches!(inline.value, b"\n" | b"\r\n") && inline.span.start.offset > 0)
}
