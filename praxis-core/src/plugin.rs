use std::path::Path;

use crate::types::{Dependency, FileEntry, Symbol};

/// Defines the interface for language-specific code analysis.
///
/// Each language plugin implements this trait to provide symbol extraction,
/// dependency parsing, and file summarization for its supported file types.
pub trait LanguageAnalyzer: Send + Sync {
    /// Returns file extensions handled by this plugin.
    fn extensions(&self) -> &[&str];

    /// Extracts all symbols (public and private) from a single file.
    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol>;

    /// Reads the repository manifest and returns declared dependencies.
    fn extract_dependencies(&self, repo_root: &Path) -> Vec<Dependency>;

    /// Returns a short summary of the file for summary-only inclusion mode.
    ///
    /// Implementations should return the leading doc comment or first meaningful
    /// comment block, capped at 300 characters. Returns [`None`] if nothing useful is found.
    fn summarize_file(&self, file: &FileEntry) -> Option<String>;
}

/// Maps file extensions to their corresponding language analyzers.
pub struct PluginRegistry {
    plugins: Vec<Box<dyn LanguageAnalyzer>>,
}

impl PluginRegistry {
    /// Creates an empty [`PluginRegistry`].
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Registers a language analyzer plugin.
    pub fn register(&mut self, plugin: Box<dyn LanguageAnalyzer>) {
        self.plugins.push(plugin);
    }

    /// Finds the plugin that handles the given file extension.
    ///
    /// Returns [`None`] if no registered plugin handles the extension.
    pub fn find_by_extension(&self, ext: &str) -> Option<&dyn LanguageAnalyzer> {
        let ext = ext.to_lowercase();
        let mut found = None;
        for plugin in &self.plugins {
            for supported in plugin.extensions() {
                if *supported == ext {
                    found = Some(plugin.as_ref());
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    }

    /// Returns all registered plugins.
    pub fn all(&self) -> &[Box<dyn LanguageAnalyzer>] {
        &self.plugins
    }
}