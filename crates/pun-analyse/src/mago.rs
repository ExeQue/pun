use mago_allocator::LocalArena;
use mago_analyzer::Analyzer;
use mago_analyzer::analysis_result::AnalysisResult;
use mago_analyzer::plugin::create_registry_with_plugins;
use mago_analyzer::settings::Settings;
use mago_codex::reference::SymbolReferences;
use mago_codex::ttype::TType;
use mago_database::file::{File, FileType};
use mago_names::ResolvedNames;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use mago_syntax::walker::MutWalker;

use crate::codebase::{self, CouldNotReadFile, read};
use crate::laravel::{self, Laravel};
use crate::{Project, display};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MagoIssue {
    pub file: String,
    pub line: u32,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dumped {
    pub file: String,
    pub line: u32,
    pub type_id: String,
}

#[derive(Debug, Default)]
pub struct MagoRun {
    pub issues: Vec<MagoIssue>,
    pub dumped: Vec<Dumped>,
}

pub fn run_mago(project: &Project, laravel: Option<Laravel>) -> Result<MagoRun, CouldNotReadFile> {
    let (codebase, _) = codebase::build(project, laravel.as_ref().map(|app| app.root))?;
    let version = codebase::php_version(project.php_version);
    let mut registry = create_registry_with_plugins(&[], false);
    if let Some(app) = laravel {
        laravel::register(&mut registry, app, project, &codebase);
    }
    let mut out = MagoRun::default();
    for path in &project.analysed {
        let Some(file) = read(path, FileType::Host)? else {
            continue;
        };
        let arena = LocalArena::new();
        let (program, names) = crate::codebase::parse(&arena, &file);
        let analyzer = Analyzer::new(
            &arena,
            &file,
            &names,
            &codebase,
            &registry,
            Settings::new(version),
        );
        let mut result = AnalysisResult::new(SymbolReferences::new());
        let analysed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            analyzer.analyze_with_artifacts(program, &mut result)
        }));
        let Ok(Ok(artifacts)) = analysed else {
            continue;
        };
        let name = display(path);
        for issue in result.issues {
            let primary = issue.annotations.iter().find(|a| a.kind.is_primary());
            out.issues.push(MagoIssue {
                file: name.clone(),
                line: primary.map_or(0, |a| line(&file, a.span)),
                code: issue.code.unwrap_or_default(),
                message: issue.message.into_owned(),
            });
        }
        let mut dumps = DumpTypeCalls {
            names: &names,
            args: Vec::new(),
        };
        dumps.walk_program(program, &mut ());
        for span in dumps.args {
            let type_id = artifacts
                .get_expression_type(&span)
                .map_or_else(|| "mixed".to_string(), |t| t.get_id().to_string());
            out.dumped.push(Dumped {
                file: name.clone(),
                line: line(&file, span),
                type_id,
            });
        }
    }
    out.issues.sort();
    out.dumped.sort();
    Ok(out)
}

fn line(file: &File, span: Span) -> u32 {
    file.line_number(span.start.offset) + 1
}

struct DumpTypeCalls<'n, 'arena> {
    names: &'n ResolvedNames<'arena>,
    args: Vec<Span>,
}

impl<'ast, 'arena> MutWalker<'ast, 'arena, ()> for DumpTypeCalls<'_, 'arena> {
    fn walk_in_function_call(&mut self, call: &'ast FunctionCall<'arena>, (): &mut ()) {
        if let Expression::Identifier(id) = call.function
            && self
                .names
                .get(&id.span())
                .eq_ignore_ascii_case(b"PHPStan\\dumpType")
            && let Some(Argument::Positional(a)) = call.argument_list.arguments.first()
        {
            self.args.push(a.value.span());
        }
    }
}
