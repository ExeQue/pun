use std::path::{Path, PathBuf};

use mago_analyzer::plugin::PluginRegistry;
use mago_analyzer::settings::Settings;
use mago_codex::metadata::CodebaseMetadata;
use mago_names::ResolvedNames;
use mago_span::Span;
use mago_syntax::cst::Program;
use pun_phpstan::types::Type;

use crate::Project;

/// A PHPStan extension package, such as Larastan, as `pun analyse` runs it. Each method is one of PHPStan's
/// extension points.
pub trait AnalyseExtension: Send + Sync {
    /// PHPStan's `StubFilesExtension::getFiles()`.
    fn stub_files(&self, root: &Path) -> Vec<PathBuf>;

    /// The contents of a file that [`Self::stub_files`] named but that is not on disk, because Pun bundles it.
    fn bundled_stub(&self, path: &Path) -> Option<&'static [u8]>;

    /// The `(alias, target)` class aliases that the extension's bootstrap registers.
    fn class_aliases(&self) -> Vec<(String, String)>;

    /// Changes the scanned symbols before the codebase is populated.
    fn prepare_codebase(&self, project: &Project, codebase: &mut CodebaseMetadata);

    /// Reads what the extension needs from the populated codebase, before any file is checked.
    fn load(&mut self, project: &Project, codebase: &CodebaseMetadata);

    /// Registers one file's dynamic return type, type-specifying and expression extensions.
    fn register(&self, registry: &mut PluginRegistry, contents: &[u8], names: &ResolvedNames<'_>);

    /// Changes the analyser's settings.
    fn settings(&self, settings: &mut Settings);

    /// The expressions whose types the extension's rules read.
    fn typed_expressions<'arena>(
        &self,
        names: &ResolvedNames<'arena>,
        program: &Program<'arena>,
    ) -> Vec<Span>;

    /// PHPStan's `TypeNodeResolverExtension::resolve()`.
    fn type_node(&self, codebase: &CodebaseMetadata, name: &str, args: &[Type]) -> Option<Type>;
}
