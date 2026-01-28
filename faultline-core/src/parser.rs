//! TypeScript and JavaScript parser using SWC
//!
//! Global invariants enforced:
//! - Deterministic parsing order
//! - Formatting, comments, and whitespace must not affect results

use anyhow::Result;
use swc_common::{sync::Lrc, FileName, SourceMap, SourceFile};
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};

/// Determine the appropriate syntax configuration based on file extension
fn syntax_for_file(filename: &str) -> Syntax {
    // Check if this is a TypeScript file
    if filename.ends_with(".tsx") || filename.ends_with(".mtsx") || filename.ends_with(".ctsx") {
        // TypeScript with JSX (TSX)
        Syntax::Typescript(swc_ecma_parser::TsSyntax {
            tsx: true, // Enable JSX in TypeScript
            decorators: false, // No experimental decorators
            dts: false, // TSX files are not declaration files
            ..Default::default()
        })
    } else if filename.ends_with(".ts") || filename.ends_with(".mts") || filename.ends_with(".cts") {
        // TypeScript without JSX
        let is_dts = filename.ends_with(".d.ts");
        Syntax::Typescript(swc_ecma_parser::TsSyntax {
            tsx: false, // No JSX in plain TS
            decorators: false, // No experimental decorators
            dts: is_dts, // Enable dts mode only for .d.ts files
            ..Default::default()
        })
    } else if filename.ends_with(".jsx") || filename.ends_with(".mjsx") || filename.ends_with(".cjsx") {
        // JavaScript with JSX
        Syntax::Es(swc_ecma_parser::EsSyntax {
            jsx: true, // Enable JSX in JavaScript
            decorators: false, // No experimental decorators
            ..Default::default()
        })
    } else {
        // Plain JavaScript (for .js, .mjs, .cjs)
        Syntax::Es(swc_ecma_parser::EsSyntax {
            jsx: false, // No JSX in plain JS
            decorators: false, // No experimental decorators
            ..Default::default()
        })
    }
}

/// Parse TypeScript, JavaScript, JSX, or TSX source code into an AST module
///
/// Automatically detects file type based on extension and uses appropriate parser configuration.
///
/// Supported file types:
/// - `.ts`, `.mts`, `.cts` - TypeScript
/// - `.tsx`, `.mtsx`, `.ctsx` - TypeScript with JSX
/// - `.js`, `.mjs`, `.cjs` - JavaScript
/// - `.jsx`, `.mjsx`, `.cjsx` - JavaScript with JSX
///
/// Returns an error if parse errors occur.
pub fn parse_source(src: &str, source_map: &Lrc<SourceMap>, filename: &str) -> Result<Module> {
    // Determine syntax based on file extension
    let syntax = syntax_for_file(filename);

    // Create SourceFile for the source code
    let source_file: Lrc<SourceFile> = source_map.new_source_file(
        FileName::Custom(filename.into()).into(),
        src.to_string(),
    );

    // Create StringInput from SourceFile
    let input = StringInput::from(&*source_file);

    // Create lexer with detected syntax
    let lexer = Lexer::new(
        syntax,
        EsVersion::Es2022,
        input,
        None,
    );

    // Create parser
    let mut parser = Parser::new_from(lexer);

    // Parse module
    parser
        .parse_module()
        .map_err(|e| {
            let error_msg = e.kind().msg();
            anyhow::anyhow!("Parse error: {}", error_msg)
                .context(format!("Failed to parse source file: {}", filename))
        })
}

/// Legacy alias for backwards compatibility
///
/// Use `parse_source` instead for new code.
#[deprecated(since = "0.1.0", note = "Use parse_source instead")]
pub fn parse_typescript(src: &str, source_map: &Lrc<SourceMap>, filename: &str) -> Result<Module> {
    parse_source(src, source_map, filename)
}

#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
