//! Language detection and abstraction layer
//!
//! This module provides language-agnostic interfaces for parsing and analyzing
//! source code across multiple programming languages.

pub mod cfg_builder;
pub mod ecmascript;
pub mod function_body;
pub mod go;
pub mod parser;
pub mod rust;
pub mod span;

use std::path::Path;

pub use cfg_builder::{CfgBuilder, get_builder_for_function};
pub use ecmascript::{ECMAScriptCfgBuilder, ECMAScriptParser};
pub use function_body::FunctionBody;
pub use go::{GoCfgBuilder, GoParser};
pub use parser::{LanguageParser, ParsedModule};
pub use rust::{RustCfgBuilder, RustParser};
pub use span::SourceSpan;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// TypeScript (.ts, .mts, .cts)
    TypeScript,
    /// TypeScript with JSX (.tsx, .mtsx, .ctsx)
    TypeScriptReact,
    /// JavaScript (.js, .mjs, .cjs)
    JavaScript,
    /// JavaScript with JSX (.jsx, .mjsx, .cjsx)
    JavaScriptReact,
    /// Go (.go)
    Go,
    /// Rust (.rs)
    Rust,
}

impl Language {
    /// Detect language from file extension
    ///
    /// Returns `None` if the extension is not recognized.
    ///
    /// # Examples
    ///
    /// ```
    /// use hotspots_core::language::Language;
    ///
    /// assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
    /// assert_eq!(Language::from_extension("go"), Some(Language::Go));
    /// assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    /// assert_eq!(Language::from_extension("py"), None);
    /// ```
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            // TypeScript
            "ts" | "mts" | "cts" => Some(Language::TypeScript),
            "tsx" | "mtsx" | "ctsx" => Some(Language::TypeScriptReact),
            // JavaScript
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "jsx" | "mjsx" | "cjsx" => Some(Language::JavaScriptReact),
            // Go
            "go" => Some(Language::Go),
            // Rust
            "rs" => Some(Language::Rust),
            // Unknown
            _ => None,
        }
    }

    /// Detect language from file path
    ///
    /// Returns `None` if the file has no extension or the extension is not recognized.
    ///
    /// # Examples
    ///
    /// ```
    /// use hotspots_core::language::Language;
    /// use std::path::Path;
    ///
    /// assert_eq!(
    ///     Language::from_path(Path::new("src/main.ts")),
    ///     Some(Language::TypeScript)
    /// );
    /// assert_eq!(
    ///     Language::from_path(Path::new("main.go")),
    ///     Some(Language::Go)
    /// );
    /// assert_eq!(
    ///     Language::from_path(Path::new("lib.rs")),
    ///     Some(Language::Rust)
    /// );
    /// ```
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// Get the canonical name of the language
    ///
    /// # Examples
    ///
    /// ```
    /// use hotspots_core::language::Language;
    ///
    /// assert_eq!(Language::TypeScript.name(), "TypeScript");
    /// assert_eq!(Language::Go.name(), "Go");
    /// assert_eq!(Language::Rust.name(), "Rust");
    /// ```
    pub fn name(&self) -> &'static str {
        match self {
            Language::TypeScript => "TypeScript",
            Language::TypeScriptReact => "TypeScript React",
            Language::JavaScript => "JavaScript",
            Language::JavaScriptReact => "JavaScript React",
            Language::Go => "Go",
            Language::Rust => "Rust",
        }
    }

    /// Check if this is a TypeScript variant
    pub fn is_typescript(&self) -> bool {
        matches!(self, Language::TypeScript | Language::TypeScriptReact)
    }

    /// Check if this is a JavaScript variant
    pub fn is_javascript(&self) -> bool {
        matches!(self, Language::JavaScript | Language::JavaScriptReact)
    }

    /// Check if this is a TypeScript or JavaScript variant
    pub fn is_ecmascript(&self) -> bool {
        self.is_typescript() || self.is_javascript()
    }

    /// Get file extensions for this language
    ///
    /// Returns a list of file extensions (without the dot) that this language uses.
    pub fn extensions(&self) -> &[&'static str] {
        match self {
            Language::TypeScript => &["ts", "mts", "cts"],
            Language::TypeScriptReact => &["tsx", "mtsx", "ctsx"],
            Language::JavaScript => &["js", "mjs", "cjs"],
            Language::JavaScriptReact => &["jsx", "mjsx", "cjsx"],
            Language::Go => &["go"],
            Language::Rust => &["rs"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension_typescript() {
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("mts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("cts"), Some(Language::TypeScript));
        assert_eq!(
            Language::from_extension("tsx"),
            Some(Language::TypeScriptReact)
        );
    }

    #[test]
    fn test_from_extension_javascript() {
        assert_eq!(Language::from_extension("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("mjs"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("cjs"), Some(Language::JavaScript));
        assert_eq!(
            Language::from_extension("jsx"),
            Some(Language::JavaScriptReact)
        );
    }

    #[test]
    fn test_from_extension_go() {
        assert_eq!(Language::from_extension("go"), Some(Language::Go));
    }

    #[test]
    fn test_from_extension_rust() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    }

    #[test]
    fn test_from_extension_unknown() {
        assert_eq!(Language::from_extension("py"), None);
        assert_eq!(Language::from_extension("java"), None);
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn test_from_path() {
        assert_eq!(
            Language::from_path(Path::new("src/main.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(Path::new("src/component.tsx")),
            Some(Language::TypeScriptReact)
        );
        assert_eq!(
            Language::from_path(Path::new("main.go")),
            Some(Language::Go)
        );
        assert_eq!(
            Language::from_path(Path::new("lib.rs")),
            Some(Language::Rust)
        );
        assert_eq!(Language::from_path(Path::new("README.md")), None);
        assert_eq!(Language::from_path(Path::new("Makefile")), None);
    }

    #[test]
    fn test_name() {
        assert_eq!(Language::TypeScript.name(), "TypeScript");
        assert_eq!(Language::TypeScriptReact.name(), "TypeScript React");
        assert_eq!(Language::JavaScript.name(), "JavaScript");
        assert_eq!(Language::JavaScriptReact.name(), "JavaScript React");
        assert_eq!(Language::Go.name(), "Go");
        assert_eq!(Language::Rust.name(), "Rust");
    }

    #[test]
    fn test_is_typescript() {
        assert!(Language::TypeScript.is_typescript());
        assert!(Language::TypeScriptReact.is_typescript());
        assert!(!Language::JavaScript.is_typescript());
        assert!(!Language::Go.is_typescript());
        assert!(!Language::Rust.is_typescript());
    }

    #[test]
    fn test_is_javascript() {
        assert!(Language::JavaScript.is_javascript());
        assert!(Language::JavaScriptReact.is_javascript());
        assert!(!Language::TypeScript.is_javascript());
        assert!(!Language::Go.is_javascript());
        assert!(!Language::Rust.is_javascript());
    }

    #[test]
    fn test_is_ecmascript() {
        assert!(Language::TypeScript.is_ecmascript());
        assert!(Language::TypeScriptReact.is_ecmascript());
        assert!(Language::JavaScript.is_ecmascript());
        assert!(Language::JavaScriptReact.is_ecmascript());
        assert!(!Language::Go.is_ecmascript());
        assert!(!Language::Rust.is_ecmascript());
    }

    #[test]
    fn test_extensions() {
        assert_eq!(
            Language::TypeScript.extensions(),
            &["ts", "mts", "cts"]
        );
        assert_eq!(
            Language::TypeScriptReact.extensions(),
            &["tsx", "mtsx", "ctsx"]
        );
        assert_eq!(Language::Go.extensions(), &["go"]);
        assert_eq!(Language::Rust.extensions(), &["rs"]);
    }
}
