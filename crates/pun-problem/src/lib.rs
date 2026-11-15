use std::ops::Range;
use std::path::PathBuf;

pub mod ignore;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Safety {
    Safe,
    Unsafe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub text: String,
}

impl Edit {
    pub fn new(span: Span, text: impl Into<String>) -> Self {
        Edit {
            span,
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub safety: Safety,
    pub edits: Vec<Edit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub rule: String,
    pub path: PathBuf,
    pub span: Span,
    pub message: String,
    pub help: Option<String>,
    pub level: Level,
    pub fix: Option<Fix>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
    len: usize,
    ends: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut starts = vec![0];
        let mut ends = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let width = match bytes[i] {
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => 0,
            };
            if width == 0 {
                i += 1;
                continue;
            }
            i += width;
            ends.push(width);
            starts.push(i);
        }
        ends.push(0);
        LineIndex {
            starts,
            len: bytes.len(),
            ends,
        }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    pub fn line_col(&self, offset: usize) -> LineCol {
        let offset = offset.min(self.len);
        let line = self.starts.partition_point(|&s| s <= offset);
        LineCol {
            line,
            column: offset - self.starts[line - 1] + 1,
        }
    }

    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let index = line.checked_sub(1)?;
        let start = *self.starts.get(index)?;
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.len, |&next| next - self.ends[index]);
        Some(start..end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub source: String,
    pub applied: Vec<usize>,
    pub deferred: Vec<usize>,
    pub invalid: Vec<usize>,
}

fn conflicts(a: Span, b: Span) -> bool {
    let overlap = a.start < b.end && b.start < a.end;
    let inserts_into = |ins: Span, other: Span| {
        ins.start == ins.end && other.start <= ins.start && ins.start <= other.end
    };
    overlap || inserts_into(a, b) || inserts_into(b, a)
}

pub fn apply_fixes(source: &str, fixes: &[&Fix]) -> Applied {
    let mut accepted: Vec<&Edit> = Vec::new();
    let mut applied = Vec::new();
    let mut deferred = Vec::new();
    let mut invalid = Vec::new();
    for (index, fix) in fixes.iter().enumerate() {
        let edits = &fix.edits;
        let bad_range = edits.iter().any(|e| {
            let Span { start, end } = e.span;
            start > end
                || end > source.len()
                || !source.is_char_boundary(start)
                || !source.is_char_boundary(end)
        });
        let self_overlap = edits
            .iter()
            .enumerate()
            .any(|(i, a)| edits[i + 1..].iter().any(|b| conflicts(a.span, b.span)));
        if bad_range || self_overlap {
            invalid.push(index);
        } else if edits
            .iter()
            .any(|e| accepted.iter().any(|a| conflicts(a.span, e.span)))
        {
            deferred.push(index);
        } else {
            accepted.extend(edits);
            applied.push(index);
        }
    }
    accepted.sort_by_key(|e| e.span.start);
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0;
    for edit in accepted {
        out.push_str(&source[cursor..edit.span.start]);
        out.push_str(&edit.text);
        cursor = edit.span.end;
    }
    out.push_str(&source[cursor..]);
    Applied {
        source: out,
        applied,
        deferred,
        invalid,
    }
}
