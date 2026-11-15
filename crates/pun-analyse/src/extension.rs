use std::path::{Path, PathBuf};

use mago_codex::metadata::CodebaseMetadata;

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
    fn prepare_codebase(&self, codebase: &mut CodebaseMetadata);
}
