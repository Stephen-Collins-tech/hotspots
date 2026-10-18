//! Language detection and abstraction layer
//!
//! This module provides language-agnostic interfaces for parsing and analyzing
//! source code across multiple programming languages.

pub mod c;
pub mod cfg_builder;
pub mod csharp;
pub mod ecmascript;
pub mod function_body;
pub mod go;
pub mod java;
pub mod parser;
pub mod python;
pub mod rust;
pub mod span;
pub mod tree_sitter_utils;

use std::path::Path;

use serde::{Deserialize, Serialize};

pub use c::{CCfgBuilder, CParser};
pub use cfg_builder::{get_builder_for_function, CfgBuilder};
pub use csharp::{CSharpCfgBuilder, CSharpParser};
pub use ecmascript::{ECMAScriptCfgBuilder, ECMAScriptParser, VueParser};
pub use function_body::FunctionBody;
pub use go::{GoCfgBuilder, GoParser};
pub use java::{JavaCfgBuilder, JavaParser};
pub use parser::{LanguageParser, ParsedModule};
pub use python::{PythonCfgBuilder, PythonParser};
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
    /// Java (.java)
    Java,
    /// Python (.py, .pyw)
    Python,
    /// Rust (.rs)
    Rust,
    /// Vue Single File Component (.vue)
    Vue,
    /// C# (.cs)
    CSharp,
    /// C (.c)
    C,
    /// C header (.h)
    CHeader,
}

/// Metadata for one supported language: its canonical name and the file
/// extensions (without the leading dot) that map to it.
///
/// This is the single source of truth for name/extension lookups.
/// `Language::from_extension`, `name()`, `extensions()`, and `from_name()`
/// all derive from this table instead of each re-typing the mapping.
struct LanguageSpec {
    variant: Language,
    name: &'static str,
    extensions: &'static [&'static str],
}

/// One row per `Language` variant. `language_spec_is_exhaustive` below fails
/// to compile if a variant is ever added here without a match arm, so this
/// table can't silently fall out of sync with the enum.
const LANGUAGE_SPECS: &[LanguageSpec] = &[
    LanguageSpec {
        variant: Language::TypeScript,
        name: "TypeScript",
        extensions: &["ts", "mts", "cts"],
    },
    LanguageSpec {
        variant: Language::TypeScriptReact,
        name: "TypeScript React",
        extensions: &["tsx", "mtsx", "ctsx"],
    },
    LanguageSpec {
        variant: Language::JavaScript,
        name: "JavaScript",
        extensions: &["js", "mjs", "cjs"],
    },
    LanguageSpec {
        variant: Language::JavaScriptReact,
        name: "JavaScript React",
        extensions: &["jsx", "mjsx", "cjsx"],
    },
    LanguageSpec {
        variant: Language::Go,
        name: "Go",
        extensions: &["go"],
    },
    LanguageSpec {
        variant: Language::Java,
        name: "Java",
        extensions: &["java"],
    },
    LanguageSpec {
        variant: Language::Python,
        name: "Python",
        extensions: &["py", "pyw"],
    },
    LanguageSpec {
        variant: Language::Rust,
        name: "Rust",
        extensions: &["rs"],
    },
    LanguageSpec {
        variant: Language::Vue,
        name: "Vue",
        extensions: &["vue"],
    },
    LanguageSpec {
        variant: Language::CSharp,
        name: "C#",
        extensions: &["cs"],
    },
    LanguageSpec {
        variant: Language::C,
        name: "C",
        extensions: &["c"],
    },
    LanguageSpec {
        variant: Language::CHeader,
        name: "C Header",
        extensions: &["h"],
    },
];

/// Exhaustiveness guard: this match has no wildcard arm, so adding a
/// `Language` variant without a corresponding `LANGUAGE_SPECS` entry is a
/// compile error here, not a runtime lookup failure. Never called at
/// runtime — its only job is to fail `cargo check` when it's out of sync.
#[allow(dead_code)]
fn language_spec_is_exhaustive(lang: Language) {
    match lang {
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact
        | Language::Go
        | Language::Java
        | Language::Python
        | Language::Rust
        | Language::Vue
        | Language::CSharp
        | Language::C
        | Language::CHeader => {}
    }
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
    /// assert_eq!(Language::from_extension("py"), Some(Language::Python));
    /// assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    /// ```
    pub fn from_extension(ext: &str) -> Option<Self> {
        LANGUAGE_SPECS
            .iter()
            .find(|spec| spec.extensions.contains(&ext))
            .map(|spec| spec.variant)
    }

    /// Line-comment prefix used to recognize `hotspots-ignore` suppression
    /// comments in this language's source syntax.
    ///
    /// Every supported language uses `//` line comments except Python, which
    /// uses `#`.
    pub fn suppression_comment_prefix(&self) -> &'static str {
        match self {
            Language::Python => "#",
            _ => "//",
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
    /// assert_eq!(Language::Python.name(), "Python");
    /// assert_eq!(Language::Rust.name(), "Rust");
    /// ```
    pub fn name(&self) -> &'static str {
        LANGUAGE_SPECS
            .iter()
            .find(|spec| spec.variant == *self)
            .map(|spec| spec.name)
            .expect("every Language variant has a LANGUAGE_SPECS entry")
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
        LANGUAGE_SPECS
            .iter()
            .find(|spec| spec.variant == *self)
            .map(|spec| spec.extensions)
            .expect("every Language variant has a LANGUAGE_SPECS entry")
    }

    /// Parse from canonical name string (as returned by `name()`).
    pub fn from_name(s: &str) -> Option<Self> {
        LANGUAGE_SPECS
            .iter()
            .find(|spec| spec.name == s)
            .map(|spec| spec.variant)
    }
}

impl Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Language::from_name(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown language: {s}")))
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
    fn test_from_extension_java() {
        assert_eq!(Language::from_extension("java"), Some(Language::Java));
    }

    #[test]
    fn test_from_extension_unknown() {
        assert_eq!(Language::from_extension("cpp"), None);
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn test_from_extension_c() {
        assert_eq!(Language::from_extension("c"), Some(Language::C));
        assert_eq!(Language::from_extension("h"), Some(Language::CHeader));
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
        assert_eq!(Language::TypeScript.extensions(), &["ts", "mts", "cts"]);
        assert_eq!(
            Language::TypeScriptReact.extensions(),
            &["tsx", "mtsx", "ctsx"]
        );
        assert_eq!(Language::Go.extensions(), &["go"]);
        assert_eq!(Language::Rust.extensions(), &["rs"]);
    }
}
