//! TypeScript and JavaScript parser using SWC
//!
//! Global invariants enforced:
//! - Deterministic parsing order
//! - Formatting, comments, and whitespace must not affect results

use anyhow::Result;
use swc_common::{sync::Lrc, FileName, SourceFile, SourceMap};
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};

/// Determine the appropriate syntax configuration based on file extension
fn syntax_for_file(filename: &str) -> Syntax {
    // Check if this is a TypeScript file
    if filename.ends_with(".tsx") || filename.ends_with(".mtsx") || filename.ends_with(".ctsx") {
        // TypeScript with JSX (TSX)
        Syntax::Typescript(swc_ecma_parser::TsSyntax {
            tsx: true,
            decorators: true, // Angular and other decorator-heavy frameworks
            dts: false,
            ..Default::default()
        })
    } else if filename.ends_with(".ts") || filename.ends_with(".mts") || filename.ends_with(".cts")
    {
        // TypeScript without JSX
        let is_dts = filename.ends_with(".d.ts");
        Syntax::Typescript(swc_ecma_parser::TsSyntax {
            tsx: false,
            decorators: true, // Angular uses @Component, @Injectable, etc.
            dts: is_dts,
            ..Default::default()
        })
    } else if filename.ends_with(".jsx")
        || filename.ends_with(".mjsx")
        || filename.ends_with(".cjsx")
    {
        // JavaScript with JSX
        Syntax::Es(swc_ecma_parser::EsSyntax {
            jsx: true,
            decorators: false,
            ..Default::default()
        })
    } else {
        // Plain JavaScript (.js, .mjs, .cjs) — jsx: true is a superset of plain JS
        Syntax::Es(swc_ecma_parser::EsSyntax {
            jsx: true, // React webpack projects put JSX in .js files by convention
            decorators: false,
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
    let syntax = syntax_for_file(filename);
    match parse_with(src, source_map, filename, syntax) {
        Ok(module) => Ok(module),
        Err(first) => {
            // Flow-annotated JavaScript (`// @flow`) is not valid ECMAScript, but most of its
            // type syntax is valid TypeScript, so retry it with the TypeScript grammar.
            if is_plain_js(filename) && declares_flow(src) {
                if let Ok(module) = parse_with(src, source_map, filename, flow_fallback_syntax()) {
                    return Ok(module);
                }
                // Flow-only forms (`?T`, `{| |}`, `opaque type`, ...): rewrite them to
                // same-length TypeScript and try once more.
                let rewritten = crate::flow::flow_to_typescript(src);
                if let Ok(module) =
                    parse_with(&rewritten, source_map, filename, flow_fallback_syntax())
                {
                    return Ok(module);
                }
                // Last resort: also drop object-type spreads and the inexact `...` marker.
                let aggressive = crate::flow::flow_to_typescript_aggressive(src);
                if let Ok(module) =
                    parse_with(&aggressive, source_map, filename, flow_fallback_syntax())
                {
                    return Ok(module);
                }
            }
            Err(first)
        }
    }
}

fn is_plain_js(filename: &str) -> bool {
    [".js", ".mjs", ".cjs", ".jsx", ".mjsx", ".cjsx"]
        .iter()
        .any(|e| filename.ends_with(e))
}

/// True if the file's leading comments contain an `@flow` pragma.
fn declares_flow(src: &str) -> bool {
    let mut end = src.len().min(1024);
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    src[..end].contains("@flow")
}

fn flow_fallback_syntax() -> Syntax {
    Syntax::Typescript(swc_ecma_parser::TsSyntax {
        tsx: true,
        decorators: true,
        dts: false,
        ..Default::default()
    })
}

fn parse_with(
    src: &str,
    source_map: &Lrc<SourceMap>,
    filename: &str,
    syntax: Syntax,
) -> Result<Module> {
    let source_file: Lrc<SourceFile> =
        source_map.new_source_file(FileName::Custom(filename.into()).into(), src.to_string());
    let input = StringInput::from(&*source_file);
    let lexer = Lexer::new(syntax, EsVersion::Es2022, input, None);
    let mut parser = Parser::new_from(lexer);
    parser.parse_module().map_err(|e| {
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
