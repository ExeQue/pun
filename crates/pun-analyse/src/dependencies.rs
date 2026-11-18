use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::symbol::SymbolKind;
use mago_database::file::FileId;
use pun_phpstan::ignore::{AnalysedDependencies, Error};
use pun_phpstan::php::normalize_path;

use crate::rules::{dependency_keys, is_named_argument_call, key_symbol};

/// The collected data PHPStan 2.3.6's UnmatchedIgnoredErrorJudge reads, from the analysed files and the entries the
/// rules left in `errors` before they are resolved: TraitDeclarationCollector (the analysed files declaring a trait),
/// AnalysedFileDependencyCollector (the traits a class uses, through other traits too, from UsedTraitFilesCollector;
/// the callees of a possibly pure call statement; the whole project for a named argument a subtype may rename) and
/// AnalysedDeclarationDependencyCollector (the callees of a possibly pure declaration).
pub(crate) fn collect(
    codebase: &CodebaseMetadata,
    files: &HashMap<FileId, PathBuf>,
    analysed: &HashSet<FileId>,
    errors: &[Error],
) -> AnalysedDependencies {
    let path = |id: FileId| files.get(&id).map(|p| normalize_path(&p.to_string_lossy()));
    let mut out = AnalysedDependencies::default();
    for k in codebase.class_likes.values() {
        let Some(file) = Some(k.span.file_id)
            .filter(|id| analysed.contains(id))
            .and_then(path)
        else {
            continue;
        };
        if k.kind == SymbolKind::Trait {
            out.trait_declaring_files.insert(file);
            continue;
        }
        let mut seen = HashSet::new();
        let mut pending: Vec<&[u8]> = k.used_traits.iter().map(|t| t.as_bytes()).collect();
        while let Some(name) = pending.pop() {
            let Some(t) = (seen.insert(name.to_ascii_lowercase()))
                .then(|| codebase.get_class_like(name))
                .flatten()
            else {
                continue;
            };
            // a trait of a stub, outside the files, is outside the configured paths too: it never decides
            (out.files.entry(file.clone()).or_default()).extend(path(t.span.file_id).map(Some));
            pending.extend(t.used_traits.iter().map(|t| t.as_bytes()));
        }
    }
    let declaring = |key: &str| {
        let id = match key_symbol(key)? {
            (true, name) => codebase.get_function(name.as_bytes())?.span.file_id,
            (false, class) => codebase.get_class_like(class.as_bytes())?.span.file_id,
        };
        path(id)
    };
    for e in errors {
        let file = normalize_path(&e.file_path);
        if is_named_argument_call(e) {
            out.files.entry(file).or_default().push(None);
        } else if let Some((call, keys)) = dependency_keys(e) {
            let found = keys.into_iter().filter_map(declaring);
            if call {
                out.files.entry(file).or_default().extend(found.map(Some));
            } else {
                out.declarations.entry(file).or_default().extend(found);
            }
        }
    }
    out
}
