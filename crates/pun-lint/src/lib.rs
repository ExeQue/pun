use std::borrow::Cow;
use std::cell::OnceCell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mago_allocator::LocalArena;
use mago_database::file::{File, FileId, FileType};
use mago_span::HasSpan;
use mago_syntax::cst::{Node, NodeKind};
use mago_syntax::settings::ParserSettings;
use mago_syntax::walker::Walker;
use pun_problem::{Edit, Fix, Level, Problem, Safety, Span, apply_fixes};

pub mod rules;
pub mod syntax;
mod walk;

pub struct Context<'a> {
    pub source: &'a str,
    pub path: &'a Path,
    pub php_version: u32,
    tokens: OnceCell<Vec<rules::support::statements::Token>>,
    facts: OnceCell<Rc<rules::types::facts::Facts>>,
    dead_code_facts: OnceCell<Rc<rules::types::facts::Facts>>,
}

impl<'a> Context<'a> {
    pub fn new(source: &'a str, path: &'a Path, php_version: u32) -> Self {
        Context {
            source,
            path,
            php_version,
            tokens: OnceCell::new(),
            facts: OnceCell::new(),
            dead_code_facts: OnceCell::new(),
        }
    }

    pub(crate) fn tokens(&self) -> &[rules::support::statements::Token] {
        self.tokens
            .get_or_init(|| rules::support::statements::lex(self.source))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub span: Span,
    pub message: String,
    pub help: Option<String>,
    pub fix: Option<Fix>,
}

impl Report {
    pub fn fixed(span: Span, message: impl Into<String>, safety: Safety, edits: Vec<Edit>) -> Self {
        Report {
            span,
            message: message.into(),
            help: None,
            fix: Some(Fix { safety, edits }),
        }
    }
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;

    fn min_php_version(&self) -> u32 {
        0
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        None
    }

    fn once(&self) -> bool {
        false
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), String> {
        match options {
            serde_json::Value::Null => Ok(()),
            _ => Err("takes no options".into()),
        }
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LintError {
    Syntax {
        span: Span,
        message: String,
    },
    FixBrokeSyntax {
        rules: Vec<String>,
        span: Span,
        message: String,
    },
    NoConvergence {
        rules: Vec<String>,
    },
    Panicked {
        message: String,
    },
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LintError::Syntax { message, .. } => write!(f, "syntax error: {message}"),
            LintError::FixBrokeSyntax { rules, message, .. } => write!(
                f,
                "the fixes of {} produced a syntax error: {message}",
                rules.join(", ")
            ),
            LintError::NoConvergence { rules } => write!(
                f,
                "fixes did not settle after {MAX_ROUNDS} rounds ({})",
                rules.join(", ")
            ),
            LintError::Panicked { message } => write!(f, "a lint rule panicked: {message}"),
        }
    }
}

impl std::error::Error for LintError {}

pub const MAX_ROUNDS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixed {
    pub source: String,
    pub applied: Vec<Problem>,
    pub remaining: Vec<Problem>,
    pub rounds: usize,
}

#[derive(Default)]
struct Found {
    reports: Vec<Report>,
    rules: Vec<&'static str>,
}

pub struct Linter {
    rules: Vec<Box<dyn Rule>>,
    by_kind: Vec<Vec<(&'static str, usize)>>,
    php_version: u32,
}

impl Linter {
    pub fn new(rules: Vec<Box<dyn Rule>>, php_version: u32) -> Self {
        let rules: Vec<Box<dyn Rule>> = rules
            .into_iter()
            .filter(|r| r.min_php_version() <= php_version)
            .collect();
        let mut by_kind = vec![Vec::new(); usize::from(u8::MAX) + 1];
        for (i, rule) in rules.iter().enumerate() {
            match rule.targets() {
                Some(kinds) => kinds
                    .iter()
                    .for_each(|&k| by_kind[k as usize].push((rule.id(), i))),
                None => by_kind.iter_mut().for_each(|r| r.push((rule.id(), i))),
            }
        }
        Linter {
            rules,
            by_kind,
            php_version,
        }
    }

    pub fn rule_ids(&self) -> Vec<&'static str> {
        self.rules.iter().map(|r| r.id()).collect()
    }

    pub fn check(&self, path: &Path, source: &str) -> Result<Vec<Problem>, LintError> {
        // SAFETY: `file` and the tree parsed from it borrow `source` and are dropped before this function returns;
        // nothing built from them outlives `source`. Mago's `File` only takes a `'static` borrow.
        let contents: &'static [u8] = unsafe { &*std::ptr::from_ref::<[u8]>(source.as_bytes()) };
        let file = File {
            id: FileId::zero(),
            name: Cow::Borrowed(b""),
            path: None,
            file_type: FileType::Host,
            contents: Cow::Borrowed(contents),
            size: source.len() as u32,
            lines: Vec::new(),
        };
        let arena = LocalArena::new();
        let program = mago_syntax::parser::parse_file_borrowed_with_settings(
            &arena,
            &file,
            ParserSettings::default(),
        );
        if let Some(error) = program.errors.first() {
            return Err(LintError::Syntax {
                span: syntax::span(error.span()),
                message: error.to_string(),
            });
        }
        let ctx = Context::new(source, path, self.php_version);
        let mut state = walk::State {
            ctx: &ctx,
            ancestors: Vec::new(),
            found: Found::default(),
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            walk::Walk { linter: self }.walk_program(program, &mut state);
        }))
        .map_err(|panic| LintError::Panicked {
            message: (panic.downcast_ref::<&str>().map(ToString::to_string))
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default(),
        })?;
        let found = state.found;
        let mut found: Vec<(&'static str, Report)> =
            found.rules.into_iter().zip(found.reports).collect();
        found.sort_by_key(|(_, report)| report.span.start);
        Ok(found
            .into_iter()
            .map(|(rule, report)| Problem {
                rule: rule.to_string(),
                path: PathBuf::from(path),
                span: report.span,
                message: report.message,
                help: report.help,
                level: Level::Error,
                fix: report.fix,
            })
            .collect())
    }

    pub fn fix(&self, path: &Path, source: &str) -> Result<Fixed, LintError> {
        self.fix_with(path, source, |_, _| {})
    }

    pub fn fix_with(
        &self,
        path: &Path,
        source: &str,
        mut filter: impl FnMut(&str, &mut Vec<Problem>),
    ) -> Result<Fixed, LintError> {
        let mut current = source.to_string();
        let mut applied = Vec::new();
        let mut last_rules = Vec::new();
        let mut done: Vec<String> = Vec::new();
        let mut round = 0;
        loop {
            let mut problems = self.check(path, &current).map_err(|e| match e {
                LintError::Syntax { span, message } if round > 0 => LintError::FixBrokeSyntax {
                    rules: last_rules.clone(),
                    span,
                    message,
                },
                e => e,
            })?;
            problems.retain(|p| !done.contains(&p.rule));
            filter(&current, &mut problems);
            let with_fix: Vec<&Problem> = problems.iter().filter(|p| p.fix.is_some()).collect();
            let fixes: Vec<&Fix> = with_fix.iter().filter_map(|p| p.fix.as_ref()).collect();
            let result = apply_fixes(&current, &fixes);
            if result.applied.is_empty() {
                return Ok(Fixed {
                    source: current,
                    applied,
                    remaining: problems,
                    rounds: round,
                });
            }
            last_rules = rule_ids(result.applied.iter().map(|&i| with_fix[i]));
            if round == MAX_ROUNDS {
                return Err(LintError::NoConvergence { rules: last_rules });
            }
            done.extend(
                (last_rules.iter())
                    .filter(|id| self.rules.iter().any(|r| r.id() == id.as_str() && r.once()))
                    .cloned(),
            );
            applied.extend(result.applied.iter().map(|&i| with_fix[i].clone()));
            current = result.source;
            round += 1;
        }
    }
}

fn rule_ids<'a>(problems: impl Iterator<Item = &'a Problem>) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for p in problems {
        if !ids.contains(&p.rule) {
            ids.push(p.rule.clone());
        }
    }
    ids
}
