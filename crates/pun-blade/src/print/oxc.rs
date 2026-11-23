//! Embedded JS and CSS through oxfmt's formatters: their IR, converted to our Prettier doc so it
//! lays out in its place in the Blade document (Prettier's `textToDoc`).

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_formatter::{
    ExpressionLayout, ExpressionRoot, JsEmbeddedIn, JsFormatOptions, JsRoot, ProgramLayout, ProgramSemicolons,
    QuoteStyle,
};
use oxc_formatter_core::{DedentMode, Tag};
use oxc_formatter_core::{FormatElement, FormatSession, GroupMode, IndentWidth, InputKind, LineMode, LineWidth, PrintMode};
use oxc_formatter_css::{CssEmbeddedIn, CssFormatOptions};
use oxc_span::SourceType;

use crate::doc::{
    AlignKind, Doc, clean_doc, strip_trailing_hardline, GroupId, HARDLINE_WITHOUT_BREAK_PARENT, Ids, align, break_parent, concat, conditional_group,
    dedent, dedent_to_root, fill, group_with, hardline, if_break_group, indent, indent_if_break, line, line_suffix,
    literalline, mark_as_root, softline,
};

/// What a JS part is parsed and printed as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsKind {
    /// `__js_expression`, an object or array hugging the quotes (Alpine expressions).
    HugExpression,
    /// `__js_expression` in a group (`:attr`).
    GroupExpression,
    /// `babel` with `__isHtmlInlineEventHandler`, on its own lines when it breaks.
    Handler,
    /// `babel` with `__isHtmlInlineEventHandler`, in a group (`:attr`'s fallback).
    HandlerGrouped,
    /// A `<script>` body.
    Script { module: bool },
}

const UNIT: u8 = 4;
const WIDTH: u16 = 120;

/// `textToDoc` for JS: `None` when the code does not parse.
pub(crate) fn js_to_doc(ids: &Ids, text: &str, kind: JsKind) -> Option<Doc> {
    let mut options = JsFormatOptions {
        indent_width: IndentWidth::try_from(UNIT).ok()?,
        line_width: LineWidth::try_from(WIDTH).ok()?,
        quote_style: QuoteStyle::Single,
        ..JsFormatOptions::default()
    };
    let expression = |layout| {
        JsRoot::Expression(ExpressionRoot {
            layout,
            tailwind_classes: false,
            ends_at_double_brace: false,
            reads_as_statements: false,
            in_parens: false,
        })
    };
    let handler = |layout| JsRoot::Program { semicolons: ProgramSemicolons::LoneExpressionOmitted, layout };
    let (root, in_attribute) = match kind {
        JsKind::HugExpression => (expression(ExpressionLayout::Hug { literals: false }), true),
        JsKind::GroupExpression => (expression(ExpressionLayout::Group), true),
        JsKind::Handler => (handler(ProgramLayout::Indented), true),
        JsKind::HandlerGrouped => (handler(ProgramLayout::Bare), true),
        JsKind::Script { .. } => {
            (JsRoot::Program { semicolons: ProgramSemicolons::AsOptions, layout: ProgramLayout::Bare }, false)
        }
    };
    if !in_attribute {
        options.quote_style = QuoteStyle::Single;
    }
    let embedded_in = JsEmbeddedIn { in_html_attribute: in_attribute, ..JsEmbeddedIn::default() };
    // Babel tries JSX first, then without it (`a < b > c`).
    let types: &[SourceType] = match kind {
        JsKind::Script { module: true } => &[SourceType::mjs().with_jsx(true), SourceType::mjs()],
        _ => &[SourceType::mjs().with_jsx(true), SourceType::cjs().with_jsx(true), SourceType::cjs()],
    };
    let allocator = Allocator::default();
    let session = FormatSession::new(&allocator, InputKind::Fragment);
    for &source_type in types {
        if let Ok(ir) = oxc_formatter::format_to_ir(&session, text, source_type, options.clone(), root, embedded_in) {
            return Some(text_to_doc(Converter::new(ids).convert(&ir.ir)));
        }
    }
    None
}

/// `textToDoc` for CSS: `None` when it does not parse.
pub(crate) fn css_to_doc(ids: &Ids, text: &str, style_attribute: bool) -> Option<Doc> {
    let options = CssFormatOptions {
        indent_width: IndentWidth::try_from(UNIT).ok()?,
        line_width: LineWidth::try_from(WIDTH).ok()?,
        single_quote: oxc_formatter_css::SingleQuote::from(true),
        ..CssFormatOptions::default()
    };
    let allocator = Allocator::default();
    let session = FormatSession::new(&allocator, InputKind::Fragment);
    let embedded_in = style_attribute.then_some(CssEmbeddedIn::HtmlStyleAttribute);
    let ir = oxc_formatter_css::format_to_ir(&session, text, options, embedded_in).ok()?;
    Some(text_to_doc(Converter::new(ids).convert(&ir.ir)))
}

/// `textToDoc`'s `stripTrailingHardline(cleanDoc(doc))`.
fn text_to_doc(doc: Doc) -> Doc {
    strip_trailing_hardline(&clean_doc(&doc))
}

/// The last line break written, for the oxc printer's collapsing of a hard line right after one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LastLine {
    None,
    Hard,
    Empty,
}

enum Frame {
    Root,
    Indent,
    Align(u8),
    Prefix(&'static str),
    Dedent(DedentMode),
    Group { id: Option<GroupId>, brk: bool },
    Conditional { flat: bool, id: Option<GroupId> },
    IndentIfBreak(Option<GroupId>),
    Fill,
    Entry,
    LineSuffix,
    Labelled,
    MarkAsRoot,
}

struct Converter<'i> {
    ids: &'i Ids,
    groups: HashMap<oxc_formatter_core::GroupId, GroupId>,
    last: LastLine,
    space: bool,
}

impl<'i> Converter<'i> {
    fn new(ids: &'i Ids) -> Self {
        Self { ids, groups: HashMap::new(), last: LastLine::None, space: false }
    }

    fn id(&mut self, id: Option<oxc_formatter_core::GroupId>) -> Option<GroupId> {
        id.map(|id| *self.groups.entry(id).or_insert_with(|| self.ids.next()))
    }

    fn convert(&mut self, elements: &[FormatElement<'_>]) -> Doc {
        let mut stack: Vec<(Frame, Vec<Doc>)> = vec![(Frame::Root, Vec::new())];
        self.walk(elements, &mut stack);
        let (_, parts) = stack.pop().unwrap_or((Frame::Root, Vec::new()));
        concat(parts)
    }

    fn push(stack: &mut [(Frame, Vec<Doc>)], doc: Doc) {
        if let Some((_, parts)) = stack.last_mut() {
            parts.push(doc);
        }
    }

    fn walk(&mut self, elements: &[FormatElement<'_>], stack: &mut Vec<(Frame, Vec<Doc>)>) {
        for element in elements {
            match element {
                FormatElement::Space => {
                    // A pending space: one for a run, none at a line start.
                    if !self.space && self.last == LastLine::None {
                        self.space = true;
                        Self::push(stack, Doc::from(" "));
                    }
                }
                FormatElement::Token { text } => {
                    self.last = LastLine::None;
                    self.space = false;
                    Self::push(stack, Doc::from(&**text));
                }
                FormatElement::Text { text, .. } => {
                    self.last = LastLine::None;
                    self.space = false;
                    Self::push(stack, Doc::from(&**text));
                }
                FormatElement::Line(mode) => {
                    for doc in self.line(*mode) {
                        Self::push(stack, doc);
                    }
                }
                FormatElement::ExpandParent => Self::push(stack, break_parent()),
                FormatElement::LineSuffixBoundary => Self::push(stack, Doc::LineSuffixBoundary),
                FormatElement::Interned(interned) => self.walk(interned, stack),
                FormatElement::BestFitting(best) => {
                    let states: Vec<Doc> = best
                        .variants()
                        .iter()
                        .map(|variant| {
                            let inner = match variant {
                                [FormatElement::Tag(Tag::StartEntry), rest @ .., FormatElement::Tag(Tag::EndEntry)] => rest,
                                all => all,
                            };
                            let last = self.last;
                            let doc = self.convert(inner);
                            self.last = last;
                            doc
                        })
                        .collect();
                    self.last = LastLine::None;
                    Self::push(stack, conditional_group(states, false));
                }
                FormatElement::TailwindClass(_) | FormatElement::EmbedPlaceholder(_) => {}
                FormatElement::Tag(tag) => self.tag(tag, stack),
            }
        }
    }

    fn line(&mut self, mode: LineMode) -> Vec<Doc> {
        let prev = self.last;
        self.space = false;
        match mode {
            LineMode::SoftOrSpace => {
                self.last = LastLine::None;
                vec![line()]
            }
            LineMode::Soft => {
                self.last = LastLine::None;
                vec![softline()]
            }
            LineMode::Hard | LineMode::HardWithoutExpand => {
                if prev != LastLine::None {
                    return Vec::new();
                }
                self.last = LastLine::Hard;
                vec![if mode == LineMode::Hard { hardline() } else { HARDLINE_WITHOUT_BREAK_PARENT }]
            }
            LineMode::Empty => {
                self.last = LastLine::Empty;
                match prev {
                    LastLine::Empty => Vec::new(),
                    LastLine::Hard => vec![hardline()],
                    LastLine::None => vec![hardline(), hardline()],
                }
            }
            LineMode::ExactLineBreaks(count) => {
                self.last = LastLine::Empty;
                (0..count.get()).map(|_| hardline()).collect()
            }
            LineMode::Literal => {
                self.last = LastLine::None;
                vec![literalline()]
            }
        }
    }

    fn tag(&mut self, tag: &Tag, stack: &mut Vec<(Frame, Vec<Doc>)>) {
        let frame = match tag {
            Tag::StartIndent => Frame::Indent,
            Tag::StartAlign(a) => Frame::Align(a.count().get()),
            Tag::StartPrefix(p) => Frame::Prefix(p.text()),
            Tag::StartDedent(mode) => Frame::Dedent(*mode),
            Tag::StartGroup(g) => Frame::Group { id: self.id(g.id()), brk: g.mode() != GroupMode::Flat },
            Tag::StartConditionalContent(c) => {
                Frame::Conditional { flat: c.mode() == PrintMode::Flat, id: self.id(c.group_id()) }
            }
            Tag::StartIndentIfGroupBreaks(id) => Frame::IndentIfBreak(self.id(Some(*id))),
            Tag::StartFill => Frame::Fill,
            Tag::StartEntry => Frame::Entry,
            Tag::StartLineSuffix => Frame::LineSuffix,
            Tag::StartLabelled(_) => Frame::Labelled,
            Tag::StartMarkAsRoot => Frame::MarkAsRoot,
            _ => {
                let Some((frame, parts)) = stack.pop() else { return };
                let doc = Self::close(frame, parts);
                Self::push(stack, doc);
                return;
            }
        };
        stack.push((frame, Vec::new()));
    }

    fn close(frame: Frame, parts: Vec<Doc>) -> Doc {
        match frame {
            Frame::Root | Frame::Entry | Frame::Labelled => concat(parts),
            Frame::Indent => indent(concat(parts)),
            Frame::Align(n) => align(AlignKind::Width(usize::from(n)), concat(parts)),
            Frame::Prefix(p) => align(AlignKind::Str(p.into()), concat(parts)),
            Frame::Dedent(DedentMode::Level) => dedent(concat(parts)),
            Frame::Dedent(DedentMode::Root) => dedent_to_root(concat(parts)),
            Frame::Group { id, brk } => group_with(concat(parts), brk, id),
            Frame::Conditional { flat: true, id } => if_break_group(Doc::default(), concat(parts), id),
            Frame::Conditional { flat: false, id } => if_break_group(concat(parts), Doc::default(), id),
            Frame::IndentIfBreak(id) => indent_if_break(concat(parts), id, false),
            Frame::Fill => fill(parts),
            Frame::LineSuffix => line_suffix(concat(parts)),
            Frame::MarkAsRoot => mark_as_root(concat(parts)),
        }
    }
}
