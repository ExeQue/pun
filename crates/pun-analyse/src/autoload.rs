use std::collections::{HashMap, HashSet};

use foldhash::fast::FixedState;
use mago_names::ResolvedNames;
use mago_span::HasSpan;
use mago_syntax::cst::{Program, Statement};

use crate::project::Autoloaded;

type Map<K, V> = HashMap<K, V, FixedState>;

pub(crate) struct Index {
    psr4: Map<Box<[u8]>, Vec<usize>>,
    psr0: Map<Box<[u8]>, Vec<usize>>,
    aliases: Map<Box<[u8]>, Alias>,
    /// The last segment of every `psr4` and `psr0` key: a name can be a key only if its last segment is one of
    /// them, which rules out most of a file's words before any full name is built and looked up.
    tails: HashSet<Box<[u8]>, FixedState>,
}

struct Alias {
    declarations: Vec<usize>,
    files: Vec<usize>,
}

type Range = (usize, usize);

type Full = (Option<Range>, Range);

impl Index {
    pub(crate) fn new<'a>(
        files: impl Iterator<Item = (usize, &'a Autoloaded)>,
        aliases: &[(String, String)],
    ) -> Index {
        let mut index = Index {
            psr4: Map::default(),
            psr0: Map::default(),
            aliases: Map::default(),
            tails: HashSet::default(),
        };
        for (i, file) in files {
            let map = if file.psr0 {
                &mut index.psr0
            } else {
                &mut index.psr4
            };
            map.entry(file.class.clone().into_boxed_slice())
                .or_default()
                .push(i);
        }
        index.tails = (index.psr4.keys().chain(index.psr0.keys()))
            .map(|key| tail(key, b'\\').into())
            .collect();
        let by_name: Map<&str, usize> = aliases
            .iter()
            .enumerate()
            .map(|(i, (a, _))| (a.as_str(), i))
            .collect();
        for (i, (name, target)) in aliases.iter().enumerate() {
            let mut declarations = vec![i];
            let mut files = Vec::new();
            let mut target = target.as_str();
            loop {
                index.look_files(
                    &mut target
                        .trim_start_matches('\\')
                        .as_bytes()
                        .to_ascii_lowercase(),
                    &mut files,
                );
                if !files.is_empty() {
                    break;
                }
                let Some(&next) = by_name.get(target.trim_start_matches('\\')) else {
                    break;
                };
                if declarations.contains(&next) {
                    break;
                }
                declarations.push(next);
                target = &aliases[next].1;
            }
            index.aliases.insert(
                name.as_bytes().into(),
                Alias {
                    declarations,
                    files,
                },
            );
        }
        index
    }

    pub(crate) fn wanted(
        &self,
        code: &[u8],
        program: &Program,
        resolved: &ResolvedNames,
        out: &mut Vec<usize>,
        declarations: &mut Vec<usize>,
    ) {
        if self.psr4.is_empty() && self.psr0.is_empty() && self.aliases.is_empty() {
            return;
        }
        let namespaces: Vec<_> = if self.aliases.is_empty() {
            Vec::new()
        } else {
            program
                .statements
                .iter()
                .filter_map(|s| match s {
                    Statement::Namespace(ns) => Some(ns),
                    _ => None,
                })
                .collect()
        };
        if !self.aliases.is_empty() {
            for (start, _, name, _) in resolved.iter() {
                if !namespaces.iter().any(|ns| {
                    ns.name
                        .as_ref()
                        .is_some_and(|n| n.span().start.offset == start)
                }) {
                    self.look_alias(name, out, declarations);
                }
            }
        }
        let names = Names::of(code);
        let mut aliases: Map<Vec<u8>, Vec<Full>> = Map::default();
        for &(alias, full) in &names.aliases {
            aliases
                .entry(code[alias.0..alias.1].to_ascii_lowercase())
                .or_default()
                .push(full);
        }
        for doc in program
            .trivia
            .iter()
            .filter(|t| !self.aliases.is_empty() && t.kind.is_docblock())
        {
            let namespace = namespaces
                .iter()
                .find(|ns| {
                    ns.span().start.offset <= doc.span.start.offset
                        && doc.span.end.offset <= ns.span().end.offset
                })
                .and_then(|ns| ns.name.as_ref());
            for (range, fq) in Names::of(doc.value).tokens {
                let token = &doc.value[range.0..range.1];
                let mut name = Vec::new();
                if !fq && let Some(ns) = namespace {
                    push_name(&mut name, ns.value());
                    name.push(b'\\');
                }
                push_name(&mut name, token);
                self.look_alias(&name, out, declarations);
                let first = token
                    .iter()
                    .position(|&b| b == b'\\')
                    .unwrap_or(token.len());
                if !fq && let Some(fulls) = aliases.get(&token[..first].to_ascii_lowercase()) {
                    for &full in fulls {
                        name.clear();
                        push_full(&mut name, code, full);
                        push_name(&mut name, &token[first..]);
                        self.look_alias(&name, out, declarations);
                    }
                }
            }
        }
        let mut first_segment = Vec::new();
        let mut last = Vec::new();
        let mut seen: HashSet<(bool, &[u8]), FixedState> =
            HashSet::with_capacity_and_hasher(names.tokens.len(), FixedState::default());
        let mut name = Vec::new();
        for &(prefix, token) in &names.grouped {
            name.clear();
            push_full(&mut name, code, (Some(prefix), token));
            self.look(&mut name, out);
        }
        for &(range, fq) in &names.tokens {
            let token = &code[range.0..range.1];
            if !seen.insert((fq, token)) {
                continue;
            }
            // The token and the namespaced names end with its last segment, which the psr-0 lookup also splits at
            // underscores.
            last.clear();
            last.extend(tail(token, b'\\').iter().map(u8::to_ascii_lowercase));
            if self.tails.contains(&last[..]) || self.tails.contains(tail(&last, b'_')) {
                name.clear();
                push_name(&mut name, token);
                self.look(&mut name, out);
                if !fq {
                    for &ns in &names.namespaces {
                        name.clear();
                        push_name(&mut name, &code[ns.0..ns.1]);
                        name.push(b'\\');
                        push_name(&mut name, token);
                        self.look(&mut name, out);
                    }
                }
            }
            if fq {
                continue;
            }
            let first = token
                .iter()
                .position(|&b| b == b'\\')
                .unwrap_or(token.len());
            first_segment.clear();
            first_segment.extend(token[..first].iter().map(u8::to_ascii_lowercase));
            if let Some(fulls) = aliases.get(&first_segment) {
                for &full in fulls {
                    name.clear();
                    push_full(&mut name, code, full);
                    push_name(&mut name, &token[first..]);
                    self.look(&mut name, out);
                }
            }
        }
    }

    fn look_alias(&self, name: &[u8], out: &mut Vec<usize>, declarations: &mut Vec<usize>) {
        if let Some(alias) = self.aliases.get(name) {
            out.extend(&alias.files);
            declarations.extend(&alias.declarations);
        }
    }

    fn look(&self, name: &mut [u8], out: &mut Vec<usize>) {
        name.make_ascii_lowercase();
        self.look_files(name, out);
    }

    fn look_files(&self, name: &mut [u8], out: &mut Vec<usize>) {
        if let Some(files) = self.psr4.get(&*name) {
            out.extend(files);
        }
        let last = name.iter().rposition(|&b| b == b'\\').map_or(0, |i| i + 1);
        for b in &mut name[last..] {
            if *b == b'_' {
                *b = b'\\';
            }
        }
        if let Some(files) = self.psr0.get(&*name) {
            out.extend(files);
        }
    }
}

/// What follows the last `separator` in `name`, or all of it.
fn tail(name: &[u8], separator: u8) -> &[u8] {
    &name[name
        .iter()
        .rposition(|&b| b == separator)
        .map_or(0, |i| i + 1)..]
}

fn push_name(name: &mut Vec<u8>, token: &[u8]) {
    let mut previous = 0;
    for &b in token {
        if !(b == b'\\' && previous == b'\\') {
            name.push(b);
        }
        previous = b;
    }
}

fn push_full(name: &mut Vec<u8>, code: &[u8], (prefix, token): Full) {
    if let Some(p) = prefix {
        push_name(name, &code[p.0..p.1]);
        name.push(b'\\');
    }
    push_name(name, &code[token.0..token.1]);
}

struct Names {
    tokens: Vec<(Range, bool)>,
    namespaces: Vec<Range>,
    aliases: Vec<(Range, Full)>,
    grouped: Vec<(Range, Range)>,
}

const START: [u64; 4] = {
    let mut t = [0; 4];
    let mut b = 0;
    while b < 256 {
        if (b as u8).is_ascii_alphabetic() || b == b'_' as usize || b >= 0x80 {
            t[b / 64] |= 1 << (b % 64);
        }
        b += 1;
    }
    t
};

fn is_start(b: u8) -> bool {
    START[b as usize / 64] & (1 << (b % 64)) != 0
}

fn is_part(b: u8) -> bool {
    is_start(b) || b.is_ascii_digit()
}

impl Names {
    fn of(code: &[u8]) -> Names {
        let mut names = Names {
            tokens: Vec::new(),
            namespaces: Vec::new(),
            aliases: Vec::new(),
            grouped: Vec::new(),
        };
        let mut group: Option<(Range, usize)> = None;
        let mut previous: [Option<Full>; 2] = [None, None];
        let mut i = 0;
        while i < code.len() {
            match code[i..].iter().position(|&b| is_start(b)) {
                Some(k) => i += k,
                None => break,
            }
            let at = |k: usize| i.checked_sub(k).map(|j| code[j]);
            let member = at(1) == Some(b'$') || (at(1) == Some(b'>') && at(2) == Some(b'-'));
            let fq = at(1) == Some(b'\\');
            let start = i;
            let mut end;
            let mut trailing = false;
            loop {
                i += code[i..]
                    .iter()
                    .position(|&b| !is_part(b))
                    .unwrap_or(code.len() - i);
                end = i;
                let sep = code[i..]
                    .iter()
                    .take(2)
                    .take_while(|&&b| b == b'\\')
                    .count();
                if sep == 0 {
                    break;
                }
                i += sep;
                if !code.get(i).is_some_and(|&b| is_start(b)) {
                    trailing = true;
                    break;
                }
            }
            if member {
                continue;
            }
            let range = (start, end);
            if group.is_some_and(|(_, close)| i > close) {
                group = None;
            }
            if trailing {
                let open = code[i..]
                    .iter()
                    .position(|b| !b.is_ascii_whitespace())
                    .map(|k| i + k)
                    .filter(|&k| code[k] == b'{');
                if let Some(open) = open
                    && let Some(close) = code[open..].iter().position(|&b| b == b'}')
                {
                    group = Some((range, open + close));
                }
                continue;
            }
            let text = |r: Option<Full>| r.map(|(_, r)| &code[r.0..r.1]);
            let prefix = group.map(|(prefix, _)| prefix);
            let full = (prefix, range);
            if text(previous[1]).is_some_and(|t| t.eq_ignore_ascii_case(b"namespace")) {
                names.namespaces.push(range);
            }
            if text(previous[1]).is_some_and(|t| t.eq_ignore_ascii_case(b"as"))
                && let Some(aliased) = previous[0]
            {
                names.aliases.push((range, aliased));
            }
            let last = code[start..end]
                .iter()
                .rposition(|&b| b == b'\\')
                .map_or(start, |k| start + k + 1);
            if last > start || prefix.is_some() {
                names.aliases.push(((last, end), full));
            }
            if let Some(prefix) = prefix {
                names.grouped.push((prefix, range));
            }
            names.tokens.push((range, fq));
            previous = [previous[1], Some(full)];
        }
        names
    }
}
