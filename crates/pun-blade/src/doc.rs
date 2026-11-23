//! A port of Prettier's document IR and printer (`src/document`): builders, `propagateBreaks`,
//! `fits` and `printDocToString`, plus the utilities the Blade printer uses.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Break {
    No,
    Yes,
    Propagated,
}

#[derive(Debug)]
pub(crate) struct Group {
    pub(crate) id: Option<GroupId>,
    pub(crate) contents: Doc,
    pub(crate) brk: Cell<Break>,
    pub(crate) expanded_states: Option<Vec<Doc>>,
}

impl Group {
    pub(crate) fn is_broken(&self) -> bool {
        self.brk.get() != Break::No
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GroupId(pub(crate) u32);

#[derive(Debug, Clone)]
pub(crate) enum AlignKind {
    Width(usize),
    Dedent,
    DedentToRoot,
    MarkAsRoot,
    Str(Rc<str>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) soft: bool,
    pub(crate) hard: bool,
    pub(crate) literal: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Doc {
    Str(Rc<str>),
    Array(Rc<Vec<Doc>>),
    Indent(Rc<Doc>),
    Align(AlignKind, Rc<Doc>),
    Group(Rc<Group>),
    Fill(Rc<Vec<Doc>>),
    IfBreak { brk: Rc<Doc>, flat: Rc<Doc>, group_id: Option<GroupId> },
    IndentIfBreak { contents: Rc<Doc>, group_id: Option<GroupId>, negate: bool },
    LineSuffix(Rc<Doc>),
    LineSuffixBoundary,
    Line(Line),
    BreakParent,
    Trim,
    Label(Rc<str>, Rc<Doc>),
    Cursor,
}

impl Default for Doc {
    fn default() -> Self {
        Doc::Str(Rc::from(""))
    }
}

impl From<&str> for Doc {
    fn from(s: &str) -> Self {
        Doc::Str(Rc::from(s))
    }
}

impl From<String> for Doc {
    fn from(s: String) -> Self {
        Doc::Str(Rc::from(s))
    }
}

impl From<&String> for Doc {
    fn from(s: &String) -> Self {
        Doc::Str(Rc::from(s.as_str()))
    }
}

impl From<Vec<Doc>> for Doc {
    fn from(v: Vec<Doc>) -> Self {
        Doc::Array(Rc::new(v))
    }
}

impl Doc {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Doc::Str(s) => Some(s),
            _ => None,
        }
    }

    /// JavaScript truthiness of a doc: only the empty string is falsy.
    pub(crate) fn is_empty_str(&self) -> bool {
        matches!(self, Doc::Str(s) if s.is_empty())
    }

    pub(crate) fn is_array(&self) -> bool {
        matches!(self, Doc::Array(_))
    }
}

// ---------------------------------------------------------------------------------------------
// Builders

pub(crate) fn text(s: impl Into<Doc>) -> Doc {
    s.into()
}

pub(crate) fn empty() -> Doc {
    Doc::default()
}

pub(crate) fn concat(parts: Vec<Doc>) -> Doc {
    Doc::Array(Rc::new(parts))
}

pub(crate) fn indent(contents: impl Into<Doc>) -> Doc {
    Doc::Indent(Rc::new(contents.into()))
}

pub(crate) fn align(kind: AlignKind, contents: impl Into<Doc>) -> Doc {
    Doc::Align(kind, Rc::new(contents.into()))
}

pub(crate) fn dedent_to_root(contents: impl Into<Doc>) -> Doc {
    align(AlignKind::DedentToRoot, contents)
}

pub(crate) fn mark_as_root(contents: impl Into<Doc>) -> Doc {
    align(AlignKind::MarkAsRoot, contents)
}

pub(crate) fn dedent(contents: impl Into<Doc>) -> Doc {
    align(AlignKind::Dedent, contents)
}

pub(crate) fn group(contents: impl Into<Doc>) -> Doc {
    group_with(contents, false, None)
}

pub(crate) fn group_with(contents: impl Into<Doc>, should_break: bool, id: Option<GroupId>) -> Doc {
    Doc::Group(Rc::new(Group {
        id,
        contents: contents.into(),
        brk: Cell::new(if should_break { Break::Yes } else { Break::No }),
        expanded_states: None,
    }))
}

pub(crate) fn conditional_group(states: Vec<Doc>, should_break: bool) -> Doc {
    Doc::Group(Rc::new(Group {
        id: None,
        contents: states[0].clone(),
        brk: Cell::new(if should_break { Break::Yes } else { Break::No }),
        expanded_states: Some(states),
    }))
}

pub(crate) fn fill(parts: Vec<Doc>) -> Doc {
    Doc::Fill(Rc::new(parts))
}

pub(crate) fn if_break(brk: impl Into<Doc>, flat: impl Into<Doc>) -> Doc {
    Doc::IfBreak { brk: Rc::new(brk.into()), flat: Rc::new(flat.into()), group_id: None }
}

pub(crate) fn if_break_group(brk: impl Into<Doc>, flat: impl Into<Doc>, group_id: Option<GroupId>) -> Doc {
    Doc::IfBreak { brk: Rc::new(brk.into()), flat: Rc::new(flat.into()), group_id }
}

pub(crate) fn indent_if_break(contents: impl Into<Doc>, group_id: Option<GroupId>, negate: bool) -> Doc {
    Doc::IndentIfBreak { contents: Rc::new(contents.into()), group_id, negate }
}

pub(crate) fn join(sep: &Doc, docs: Vec<Doc>) -> Doc {
    let mut parts = Vec::with_capacity(docs.len() * 2);
    for (i, d) in docs.into_iter().enumerate() {
        if i != 0 {
            parts.push(sep.clone());
        }
        parts.push(d);
    }
    concat(parts)
}

pub(crate) fn label(name: &str, contents: impl Into<Doc>) -> Doc {
    let contents = contents.into();
    if name.is_empty() { contents } else { Doc::Label(Rc::from(name), Rc::new(contents)) }
}

pub(crate) fn line_suffix(contents: impl Into<Doc>) -> Doc {
    Doc::LineSuffix(Rc::new(contents.into()))
}

pub(crate) const LINE: Doc = Doc::Line(Line { soft: false, hard: false, literal: false });
pub(crate) const SOFTLINE: Doc = Doc::Line(Line { soft: true, hard: false, literal: false });
pub(crate) const HARDLINE_WITHOUT_BREAK_PARENT: Doc = Doc::Line(Line { soft: false, hard: true, literal: false });
pub(crate) const LITERALLINE_WITHOUT_BREAK_PARENT: Doc = Doc::Line(Line { soft: false, hard: true, literal: true });

pub(crate) fn line() -> Doc {
    LINE
}

pub(crate) fn softline() -> Doc {
    SOFTLINE
}

pub(crate) fn hardline() -> Doc {
    concat(vec![HARDLINE_WITHOUT_BREAK_PARENT, Doc::BreakParent])
}

pub(crate) fn literalline() -> Doc {
    concat(vec![LITERALLINE_WITHOUT_BREAK_PARENT, Doc::BreakParent])
}

pub(crate) fn break_parent() -> Doc {
    Doc::BreakParent
}

/// Hands out group ids (Prettier's `Symbol(...)`).
#[derive(Debug, Default)]
pub(crate) struct Ids(Cell<u32>);

impl Ids {
    pub(crate) fn next(&self) -> GroupId {
        let v = self.0.get() + 1;
        self.0.set(v);
        GroupId(v)
    }
}

// ---------------------------------------------------------------------------------------------
// Utilities

fn traverse(doc: &Doc, on_enter: &mut dyn FnMut(&Doc) -> bool, on_exit: Option<&mut dyn FnMut(&Doc)>, conditional_groups: bool) {
    enum Item<'a> {
        Enter(&'a Doc),
        Exit(&'a Doc),
    }
    let mut on_exit = on_exit;
    let mut stack = vec![Item::Enter(doc)];
    while let Some(item) = stack.pop() {
        let d = match item {
            Item::Exit(d) => {
                if let Some(f) = on_exit.as_mut() {
                    f(d);
                }
                continue;
            }
            Item::Enter(d) => d,
        };
        if on_exit.is_some() {
            stack.push(Item::Exit(d));
        }
        if !on_enter(d) {
            continue;
        }
        match d {
            Doc::Array(parts) | Doc::Fill(parts) => {
                for p in parts.iter().rev() {
                    stack.push(Item::Enter(p));
                }
            }
            Doc::IfBreak { brk, flat, .. } => {
                stack.push(Item::Enter(flat));
                stack.push(Item::Enter(brk));
            }
            Doc::Group(g) => {
                if conditional_groups && let Some(states) = &g.expanded_states {
                    for s in states.iter().rev() {
                        stack.push(Item::Enter(s));
                    }
                } else {
                    stack.push(Item::Enter(&g.contents));
                }
            }
            Doc::Align(_, c) | Doc::Indent(c) | Doc::IndentIfBreak { contents: c, .. } | Doc::Label(_, c) | Doc::LineSuffix(c) => {
                stack.push(Item::Enter(c));
            }
            _ => {}
        }
    }
}

fn find_in_doc(doc: &Doc, f: impl Fn(&Doc) -> Option<bool>) -> bool {
    let mut result = false;
    let mut done = false;
    traverse(
        doc,
        &mut |d| {
            if done {
                return false;
            }
            if let Some(r) = f(d) {
                done = true;
                result = r;
            }
            true
        },
        None,
        false,
    );
    result
}

pub(crate) fn will_break(doc: &Doc) -> bool {
    find_in_doc(doc, |d| match d {
        Doc::Group(g) if g.is_broken() => Some(true),
        Doc::Line(l) if l.hard => Some(true),
        Doc::BreakParent => Some(true),
        _ => None,
    })
}

pub(crate) fn can_break(doc: &Doc) -> bool {
    find_in_doc(doc, |d| matches!(d, Doc::Line(_)).then_some(true))
}

pub(crate) fn propagate_breaks(doc: &Doc) {
    fn break_parent_group(stack: &[Rc<Group>]) {
        if let Some(parent) = stack.last()
            && parent.expanded_states.is_none()
            && !parent.is_broken()
        {
            parent.brk.set(Break::Propagated);
        }
    }
    let visited: std::cell::RefCell<std::collections::HashSet<*const Group>> = std::cell::RefCell::default();
    let stack: std::cell::RefCell<Vec<Rc<Group>>> = std::cell::RefCell::default();
    traverse(
        doc,
        &mut |d| {
            if matches!(d, Doc::BreakParent) {
                break_parent_group(&stack.borrow());
            }
            if let Doc::Group(g) = d {
                stack.borrow_mut().push(g.clone());
                if !visited.borrow_mut().insert(Rc::as_ptr(g)) {
                    return false;
                }
            }
            true
        },
        Some(&mut |d| {
            if matches!(d, Doc::Group(_)) {
                let g = stack.borrow_mut().pop();
                if g.is_some_and(|g| g.is_broken()) {
                    break_parent_group(&stack.borrow());
                }
            }
        }),
        true,
    );
}

/// `mapDoc`: rebuilds the doc bottom-up, keeping shared groups shared.
pub(crate) fn map_doc(doc: &Doc, cb: &mut dyn FnMut(Doc) -> Doc) -> Doc {
    let mut memo: HashMap<*const Group, Doc> = HashMap::new();
    map_rec(doc, cb, &mut memo)
}

fn map_rec(doc: &Doc, cb: &mut dyn FnMut(Doc) -> Doc, memo: &mut HashMap<*const Group, Doc>) -> Doc {
    let mapped = match doc {
        Doc::Array(parts) => Doc::Array(Rc::new(parts.iter().map(|p| map_rec(p, cb, memo)).collect())),
        Doc::Fill(parts) => Doc::Fill(Rc::new(parts.iter().map(|p| map_rec(p, cb, memo)).collect())),
        Doc::IfBreak { brk, flat, group_id } => Doc::IfBreak {
            brk: Rc::new(map_rec(brk, cb, memo)),
            flat: Rc::new(map_rec(flat, cb, memo)),
            group_id: *group_id,
        },
        Doc::Group(g) => {
            let key = Rc::as_ptr(g);
            if let Some(d) = memo.get(&key) {
                return d.clone();
            }
            let (contents, expanded_states) = if let Some(states) = &g.expanded_states {
                let states: Vec<Doc> = states.iter().map(|s| map_rec(s, cb, memo)).collect();
                (states[0].clone(), Some(states))
            } else {
                (map_rec(&g.contents, cb, memo), None)
            };
            let out = cb(Doc::Group(Rc::new(Group { id: g.id, contents, brk: Cell::new(g.brk.get()), expanded_states })));
            memo.insert(key, out.clone());
            return out;
        }
        Doc::Align(k, c) => Doc::Align(k.clone(), Rc::new(map_rec(c, cb, memo))),
        Doc::Indent(c) => Doc::Indent(Rc::new(map_rec(c, cb, memo))),
        Doc::IndentIfBreak { contents, group_id, negate } => {
            Doc::IndentIfBreak { contents: Rc::new(map_rec(contents, cb, memo)), group_id: *group_id, negate: *negate }
        }
        Doc::Label(l, c) => Doc::Label(l.clone(), Rc::new(map_rec(c, cb, memo))),
        Doc::LineSuffix(c) => Doc::LineSuffix(Rc::new(map_rec(c, cb, memo))),
        other => other.clone(),
    };
    cb(mapped)
}

fn clean_doc_fn(doc: Doc) -> Doc {
    match &doc {
        Doc::Fill(parts) => {
            if parts.iter().all(Doc::is_empty_str) {
                return empty();
            }
            if parts.len() == 1 {
                return parts[0].clone();
            }
        }
        Doc::Group(g) => {
            if g.contents.is_empty_str() && g.id.is_none() && !g.is_broken() && g.expanded_states.is_none() {
                return empty();
            }
            if let Doc::Group(inner) = &g.contents
                && inner.id == g.id
                && inner.brk.get() == g.brk.get()
                && inner.expanded_states.is_none()
                && g.expanded_states.is_none()
            {
                return g.contents.clone();
            }
        }
        Doc::Align(_, c) | Doc::Indent(c) | Doc::IndentIfBreak { contents: c, .. } | Doc::LineSuffix(c) => {
            if c.is_empty_str() {
                return empty();
            }
        }
        Doc::IfBreak { brk, flat, .. } => {
            if brk.is_empty_str() && flat.is_empty_str() {
                return empty();
            }
        }
        Doc::Array(items) => {
            let mut parts: Vec<Doc> = Vec::new();
            for part in items.iter() {
                if part.is_empty_str() {
                    continue;
                }
                let (current, rest): (Doc, Vec<Doc>) = match part {
                    Doc::Array(inner) => {
                        if inner.is_empty() {
                            // `[currentPart] = []` pushes undefined; an empty array part prints nothing.
                            continue;
                        }
                        (inner[0].clone(), inner[1..].to_vec())
                    }
                    other => (other.clone(), Vec::new()),
                };
                if let (Doc::Str(cur), Some(Doc::Str(last))) = (&current, parts.last()) {
                    let joined = format!("{last}{cur}");
                    let n = parts.len();
                    parts[n - 1] = Doc::from(joined);
                } else {
                    parts.push(current);
                }
                parts.extend(rest);
            }
            if parts.is_empty() {
                return empty();
            }
            if parts.len() == 1 {
                return parts.pop().unwrap_or_default();
            }
            return concat(parts);
        }
        _ => {}
    }
    doc
}

pub(crate) fn clean_doc(doc: &Doc) -> Doc {
    map_doc(doc, &mut clean_doc_fn)
}

fn trim_newlines_end(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

fn strip_trailing_hardline_from_parts(parts: &[Doc]) -> Vec<Doc> {
    let mut parts = parts.to_vec();
    while parts.len() >= 2
        && matches!(parts[parts.len() - 2], Doc::Line(_))
        && matches!(parts[parts.len() - 1], Doc::BreakParent)
    {
        parts.truncate(parts.len() - 2);
    }
    if let Some(last) = parts.last_mut() {
        *last = strip_trailing_hardline_from_doc(last);
    }
    parts
}

fn strip_trailing_hardline_from_doc(doc: &Doc) -> Doc {
    match doc {
        Doc::Indent(c) => indent(strip_trailing_hardline_from_doc(c)),
        Doc::IndentIfBreak { contents, group_id, negate } => {
            Doc::IndentIfBreak { contents: Rc::new(strip_trailing_hardline_from_doc(contents)), group_id: *group_id, negate: *negate }
        }
        Doc::Group(g) => Doc::Group(Rc::new(Group {
            id: g.id,
            contents: strip_trailing_hardline_from_doc(&g.contents),
            brk: Cell::new(g.brk.get()),
            expanded_states: g.expanded_states.clone(),
        })),
        Doc::LineSuffix(c) => line_suffix(strip_trailing_hardline_from_doc(c)),
        Doc::Label(l, c) => Doc::Label(l.clone(), Rc::new(strip_trailing_hardline_from_doc(c))),
        Doc::IfBreak { brk, flat, group_id } => Doc::IfBreak {
            brk: Rc::new(strip_trailing_hardline_from_doc(brk)),
            flat: Rc::new(strip_trailing_hardline_from_doc(flat)),
            group_id: *group_id,
        },
        Doc::Fill(parts) => Doc::Fill(Rc::new(strip_trailing_hardline_from_parts(parts))),
        Doc::Array(parts) => concat(strip_trailing_hardline_from_parts(parts)),
        Doc::Str(s) => Doc::from(trim_newlines_end(s)),
        other => other.clone(),
    }
}

pub(crate) fn strip_trailing_hardline(doc: &Doc) -> Doc {
    strip_trailing_hardline_from_doc(&clean_doc(doc))
}

/// `replaceEndOfLine(doc, literalline)`.
pub(crate) fn replace_end_of_line(doc: &Doc) -> Doc {
    replace_end_of_line_with(doc, &literalline())
}

pub(crate) fn replace_end_of_line_with(doc: &Doc, replacement: &Doc) -> Doc {
    map_doc(doc, &mut |d| match &d {
        Doc::Str(s) if s.contains('\n') => join(replacement, s.split('\n').map(Doc::from).collect()),
        Doc::Str(_) => d,
        _ => d,
    })
}

// ---------------------------------------------------------------------------------------------
// String width (`getStringWidth`)

const FULLWIDTH_RANGES: &[(u32, u32)] = &[(12288, 12288), (65281, 65376), (65504, 65510)];
const WIDE_RANGES: &[(u32, u32)] = &[(4352, 4447), (8986, 8987), (9001, 9002), (9193, 9196), (9200, 9200), (9203, 9203), (9725, 9726), (9748, 9749), (9776, 9783), (9800, 9811), (9855, 9855), (9866, 9871), (9875, 9875), (9889, 9889), (9898, 9899), (9917, 9918), (9924, 9925), (9934, 9934), (9940, 9940), (9962, 9962), (9970, 9971), (9973, 9973), (9978, 9978), (9981, 9981), (9989, 9989), (9994, 9995), (10024, 10024), (10060, 10060), (10062, 10062), (10067, 10069), (10071, 10071), (10133, 10135), (10160, 10160), (10175, 10175), (11035, 11036), (11088, 11088), (11093, 11093), (11904, 11929), (11931, 12019), (12032, 12245), (12272, 12287), (12289, 12350), (12353, 12438), (12441, 12543), (12549, 12591), (12593, 12686), (12688, 12773), (12783, 12830), (12832, 12871), (12880, 42124), (42128, 42182), (43360, 43388), (44032, 55203), (63744, 64255), (65040, 65049), (65072, 65106), (65108, 65126), (65128, 65131), (94176, 94180), (94192, 94198), (94208, 101594), (101631, 101664), (101760, 101874), (101888, 102801), (102816, 102866), (110576, 110579), (110581, 110587), (110589, 110590), (110592, 110888), (110898, 110898), (110928, 110930), (110933, 110933), (110948, 110952), (110960, 111355), (119552, 119638), (119648, 119670), (126980, 126980), (127183, 127183), (127374, 127374), (127377, 127386), (127406, 127406), (127488, 127490), (127504, 127547), (127552, 127560), (127568, 127569), (127584, 127589), (127744, 127776), (127789, 127797), (127799, 127868), (127870, 127891), (127904, 127946), (127951, 127955), (127968, 127984), (127988, 127988), (127992, 128062), (128064, 128064), (128066, 128252), (128255, 128317), (128331, 128334), (128336, 128359), (128378, 128378), (128405, 128406), (128420, 128420), (128507, 128591), (128640, 128709), (128716, 128716), (128720, 128722), (128725, 128729), (128732, 128735), (128747, 128748), (128756, 128764), (128986, 128986), (128992, 129003), (129008, 129008), (129292, 129338), (129340, 129349), (129351, 129535), (129648, 129660), (129664, 129734), (129736, 129736), (129740, 129757), (129759, 129771), (129775, 129786), (131072, 196605), (196608, 262141), ];

fn in_ranges(ranges: &[(u32, u32)], cp: u32) -> bool {
    ranges.binary_search_by(|&(lo, hi)| if cp < lo { std::cmp::Ordering::Greater } else if cp > hi { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal }).is_ok()
}

fn is_zero_width_mark(c: char) -> bool {
    // `\p{Nonspacing_Mark}` / `\p{Enclosing_Mark}`: the common combining blocks.
    let cp = c as u32;
    matches!(cp,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0610..=0x061A | 0x064B..=0x065F
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20F0 | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0xE0100..=0xE01EF)
}

pub(crate) fn string_width(text: &str) -> usize {
    if text.bytes().all(|b| (0x20..=0x7f).contains(&b)) {
        return text.len();
    }
    let mut width = 0;
    for c in text.chars() {
        let cp = c as u32;
        if cp <= 31 || (127..=159).contains(&cp) || is_zero_width_mark(c) || cp == 0x200D {
            continue;
        }
        width += if in_ranges(FULLWIDTH_RANGES, cp) || in_ranges(WIDE_RANGES, cp) { 2 } else { 1 };
    }
    width
}

// ---------------------------------------------------------------------------------------------
// Printer

#[derive(Debug, Clone)]
enum IndentCmd {
    Indent,
    Width(usize),
    Str(Rc<str>),
}

#[derive(Debug)]
struct Indent {
    value: String,
    length: usize,
    queue: Vec<IndentCmd>,
    /// `None` stands for Prettier's `ROOT_INDENT`.
    root: Option<Rc<Indent>>,
}

fn root_indent() -> Rc<Indent> {
    Rc::new(Indent { value: String::new(), length: 0, queue: Vec::new(), root: None })
}

fn generate_indent(ind: &Rc<Indent>, cmd: Option<IndentCmd>, tab_width: usize) -> Rc<Indent> {
    let mut queue = ind.queue.clone();
    match cmd {
        None => {
            queue.pop();
        }
        Some(c) => queue.push(c),
    }
    let mut value = String::new();
    let mut length = 0;
    let mut last_spaces = 0;
    for c in &queue {
        match c {
            IndentCmd::Indent => {
                value.push_str(&" ".repeat(last_spaces));
                length += last_spaces;
                last_spaces = 0;
                value.push_str(&" ".repeat(tab_width));
                length += tab_width;
            }
            IndentCmd::Str(s) => {
                value.push_str(&" ".repeat(last_spaces));
                length += last_spaces;
                last_spaces = 0;
                value.push_str(s);
                length += s.encode_utf16().count();
            }
            IndentCmd::Width(w) => last_spaces += w,
        }
    }
    value.push_str(&" ".repeat(last_spaces));
    length += last_spaces;
    Rc::new(Indent { value, length, queue, root: ind.root.clone() })
}

fn make_align(ind: &Rc<Indent>, kind: &AlignKind, tab_width: usize) -> Rc<Indent> {
    match kind {
        AlignKind::MarkAsRoot => Rc::new(Indent {
            value: ind.value.clone(),
            length: ind.length,
            queue: ind.queue.clone(),
            root: Some(ind.clone()),
        }),
        AlignKind::DedentToRoot => ind.root.clone().unwrap_or_else(root_indent),
        AlignKind::Width(0) => ind.clone(),
        AlignKind::Width(n) => generate_indent(ind, Some(IndentCmd::Width(*n)), tab_width),
        AlignKind::Dedent => generate_indent(ind, None, tab_width),
        AlignKind::Str(s) => {
            if s.is_empty() {
                ind.clone()
            } else {
                generate_indent(ind, Some(IndentCmd::Str(s.clone())), tab_width)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Break,
    Flat,
}

#[derive(Clone)]
struct Cmd {
    ind: Rc<Indent>,
    mode: Mode,
    doc: Doc,
    /// `DOC_FILL_PRINTED_LENGTH` for fills.
    fill_offset: usize,
}

fn cmd(ind: &Rc<Indent>, mode: Mode, doc: Doc) -> Cmd {
    Cmd { ind: ind.clone(), mode, doc, fill_offset: 0 }
}

fn trim_indentation(text: &mut String) -> usize {
    let trimmed_len = text.trim_end_matches([' ', '\t']).len();
    let count = text.len() - trimmed_len;
    text.truncate(trimmed_len);
    count
}

fn fits(
    next: (Mode, &Doc, usize),
    rest: &[Cmd],
    mut remaining: isize,
    mut has_line_suffix: bool,
    group_modes: &HashMap<GroupId, Mode>,
    must_be_flat: bool,
) -> bool {
    let mut rest_idx = rest.len();
    let mut has_pending_space = false;
    let mut commands: Vec<(Mode, Doc, usize)> = vec![(next.0, next.1.clone(), next.2)];
    let mut output = String::new();
    while remaining >= 0 {
        let Some((mode, doc, fill_offset)) = commands.pop() else {
            if rest_idx == 0 {
                return true;
            }
            rest_idx -= 1;
            let c = &rest[rest_idx];
            commands.push((c.mode, c.doc.clone(), c.fill_offset));
            continue;
        };
        match &doc {
            Doc::Str(s) => {
                if !s.is_empty() {
                    if has_pending_space {
                        output.push(' ');
                        remaining -= 1;
                        has_pending_space = false;
                    }
                    output.push_str(s);
                    remaining -= string_width(s) as isize;
                }
            }
            Doc::Array(parts) => {
                for p in parts.iter().rev() {
                    commands.push((mode, p.clone(), 0));
                }
            }
            Doc::Fill(parts) => {
                for p in parts[fill_offset.min(parts.len())..].iter().rev() {
                    commands.push((mode, p.clone(), 0));
                }
            }
            Doc::Indent(c) | Doc::Align(_, c) | Doc::IndentIfBreak { contents: c, .. } | Doc::Label(_, c) => {
                commands.push((mode, (**c).clone(), 0));
            }
            Doc::Trim => {
                remaining += trim_indentation(&mut output) as isize;
            }
            Doc::Group(g) => {
                if must_be_flat && g.is_broken() {
                    return false;
                }
                let group_mode = if g.is_broken() { Mode::Break } else { mode };
                let contents = match (&g.expanded_states, group_mode) {
                    (Some(states), Mode::Break) => states[states.len() - 1].clone(),
                    _ => g.contents.clone(),
                };
                commands.push((group_mode, contents, 0));
            }
            Doc::IfBreak { brk, flat, group_id } => {
                let group_mode = match group_id {
                    Some(id) => group_modes.get(id).copied().unwrap_or(Mode::Flat),
                    None => mode,
                };
                let contents = if group_mode == Mode::Break { brk } else { flat };
                if !contents.is_empty_str() {
                    commands.push((mode, (**contents).clone(), 0));
                }
            }
            Doc::Line(l) => {
                if mode == Mode::Break || l.hard {
                    return true;
                }
                if !l.soft {
                    has_pending_space = true;
                }
            }
            Doc::LineSuffix(_) => has_line_suffix = true,
            Doc::LineSuffixBoundary => {
                if has_line_suffix {
                    return false;
                }
            }
            Doc::BreakParent | Doc::Cursor => {}
        }
    }
    false
}

pub(crate) struct PrintOptions {
    pub(crate) print_width: usize,
    pub(crate) tab_width: usize,
}

#[allow(clippy::too_many_lines, reason = "mirrors Prettier's single printDocToString loop")]
pub(crate) fn print_doc_to_string(doc: &Doc, options: &PrintOptions) -> String {
    let mut group_modes: HashMap<GroupId, Mode> = HashMap::new();
    let width = options.print_width as isize;
    let mut pos: isize = 0;
    let root = root_indent();
    let mut commands = vec![cmd(&root, Mode::Break, doc.clone())];
    let mut should_remeasure = false;
    let mut line_suffix: Vec<Cmd> = Vec::new();
    let mut settled = String::new();
    let mut unsettled = String::new();
    propagate_breaks(doc);

    while let Some(Cmd { ind, mode, doc, fill_offset }) = commands.pop() {
        match &doc {
            Doc::Str(s) => {
                if !s.is_empty() {
                    unsettled.push_str(s);
                    if !commands.is_empty() {
                        pos += string_width(s) as isize;
                    }
                }
            }
            Doc::Array(parts) => {
                for p in parts.iter().rev() {
                    commands.push(cmd(&ind, mode, p.clone()));
                }
            }
            Doc::Cursor | Doc::BreakParent => {}
            Doc::Indent(c) => {
                commands.push(cmd(&generate_indent(&ind, Some(IndentCmd::Indent), options.tab_width), mode, (**c).clone()));
            }
            Doc::Align(k, c) => {
                commands.push(cmd(&make_align(&ind, k, options.tab_width), mode, (**c).clone()));
            }
            Doc::Trim => {
                pos -= trim_indentation(&mut unsettled) as isize;
                settled.push_str(&unsettled);
                unsettled.clear();
            }
            Doc::Group(g) => {
                let next = if mode == Mode::Flat && !should_remeasure {
                    cmd(&ind, if g.is_broken() { Mode::Break } else { Mode::Flat }, g.contents.clone())
                } else {
                    should_remeasure = false;
                    let remaining = width - pos;
                    let has_ls = !line_suffix.is_empty();
                    if !g.is_broken() && fits((Mode::Flat, &g.contents, 0), &commands, remaining, has_ls, &group_modes, false) {
                        cmd(&ind, Mode::Flat, g.contents.clone())
                    } else if let Some(states) = &g.expanded_states {
                        let mut chosen = None;
                        if !g.is_broken() {
                            for state in &states[1..states.len().saturating_sub(1).max(1)] {
                                if fits((Mode::Flat, state, 0), &commands, remaining, has_ls, &group_modes, false) {
                                    chosen = Some(cmd(&ind, Mode::Flat, state.clone()));
                                    break;
                                }
                            }
                        }
                        chosen.unwrap_or_else(|| cmd(&ind, Mode::Break, states[states.len() - 1].clone()))
                    } else {
                        cmd(&ind, Mode::Break, g.contents.clone())
                    }
                };
                if let Some(id) = g.id {
                    group_modes.insert(id, next.mode);
                }
                commands.push(next);
            }
            Doc::Fill(parts) => {
                let remaining = width - pos;
                let offset = fill_offset;
                let length = parts.len().saturating_sub(offset);
                if length == 0 {
                    continue;
                }
                let has_ls = !line_suffix.is_empty();
                let content = &parts[offset];
                let content_fits = fits((Mode::Flat, content, 0), &[], remaining, has_ls, &group_modes, true);
                if length == 1 {
                    commands.push(cmd(&ind, if content_fits { Mode::Flat } else { Mode::Break }, content.clone()));
                    continue;
                }
                let whitespace = &parts[offset + 1];
                if length == 2 {
                    let m = if content_fits { Mode::Flat } else { Mode::Break };
                    commands.push(cmd(&ind, m, whitespace.clone()));
                    commands.push(cmd(&ind, m, content.clone()));
                    continue;
                }
                let second = &parts[offset + 2];
                let pair = concat(vec![content.clone(), whitespace.clone(), second.clone()]);
                let pair_fits = fits((Mode::Flat, &pair, 0), &[], remaining, has_ls, &group_modes, true);
                commands.push(Cmd { ind: ind.clone(), mode, doc: doc.clone(), fill_offset: offset + 2 });
                if pair_fits {
                    commands.push(cmd(&ind, Mode::Flat, whitespace.clone()));
                    commands.push(cmd(&ind, Mode::Flat, content.clone()));
                } else if content_fits {
                    commands.push(cmd(&ind, Mode::Break, whitespace.clone()));
                    commands.push(cmd(&ind, Mode::Flat, content.clone()));
                } else {
                    commands.push(cmd(&ind, Mode::Break, whitespace.clone()));
                    commands.push(cmd(&ind, Mode::Break, content.clone()));
                }
            }
            Doc::IfBreak { brk, flat, group_id } => {
                let group_mode = match group_id {
                    Some(id) => group_modes.get(id).copied(),
                    None => Some(mode),
                };
                match group_mode {
                    Some(Mode::Break) if !brk.is_empty_str() => commands.push(cmd(&ind, mode, (**brk).clone())),
                    Some(Mode::Flat) if !flat.is_empty_str() => commands.push(cmd(&ind, mode, (**flat).clone())),
                    _ => {}
                }
            }
            Doc::IndentIfBreak { contents, group_id, negate } => {
                let group_mode = match group_id {
                    Some(id) => group_modes.get(id).copied(),
                    None => Some(mode),
                };
                let c = (**contents).clone();
                match group_mode {
                    Some(Mode::Break) => commands.push(cmd(&ind, mode, if *negate { c } else { indent(c) })),
                    Some(Mode::Flat) => commands.push(cmd(&ind, mode, if *negate { indent(c) } else { c })),
                    None => {}
                }
            }
            Doc::LineSuffix(c) => line_suffix.push(cmd(&ind, mode, (**c).clone())),
            Doc::LineSuffixBoundary => {
                if !line_suffix.is_empty() {
                    commands.push(cmd(&ind, mode, HARDLINE_WITHOUT_BREAK_PARENT));
                }
            }
            Doc::Line(l) => {
                let mut do_break = mode == Mode::Break;
                if mode == Mode::Flat {
                    if l.hard {
                        should_remeasure = true;
                        do_break = true;
                    } else if !l.soft {
                        unsettled.push(' ');
                        pos += 1;
                    }
                }
                if do_break {
                    if !line_suffix.is_empty() {
                        commands.push(cmd(&ind, mode, doc.clone()));
                        commands.extend(line_suffix.drain(..).rev());
                    } else if l.literal {
                        unsettled.push('\n');
                        pos = 0;
                        let r = ind.root.clone().unwrap_or_else(root_indent);
                        if !r.value.is_empty() {
                            unsettled.push_str(&r.value);
                        }
                        pos = r.length as isize;
                    } else {
                        trim_indentation(&mut unsettled);
                        settled.push_str(&unsettled);
                        unsettled.clear();
                        unsettled.push('\n');
                        unsettled.push_str(&ind.value);
                        pos = ind.length as isize;
                    }
                }
            }
            Doc::Label(_, c) => commands.push(cmd(&ind, mode, (**c).clone())),
        }
        if commands.is_empty() && !line_suffix.is_empty() {
            commands.extend(line_suffix.drain(..).rev());
        }
    }
    settled.push_str(&unsettled);
    settled
}
