use praxis_core::plugin::PluginRegistry;

/// Output format shared across CLI commands.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Json,
    Markdown,
    Both,
}

/// Creates a [`PluginRegistry`] with all built-in language analyzers.
pub fn default_plugin_registry() -> PluginRegistry {
    let mut plugins = PluginRegistry::new();
    plugins.register(Box::new(praxis_lang_angelscript::AngelScriptAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_cpp::CppAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_elixir::ElixirAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_rust::RustAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_go::GoAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_ts::TypeScriptAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_python::PythonAnalyzer::new()));
    plugins
}
