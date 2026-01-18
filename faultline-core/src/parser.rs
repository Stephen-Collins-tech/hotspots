//! TypeScript parser using SWC
//!
//! Global invariants enforced:
//! - Deterministic parsing order
//! - Formatting, comments, and whitespace must not affect results

use anyhow::Result;
use swc_common::{sync::Lrc, FileName, SourceMap, SourceFile};
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};

/// Create SWC parser syntax configuration for TypeScript only
///
/// This configuration:
/// - Enables TypeScript syntax
/// - Disables JSX (will error on JSX syntax)
/// - Disables experimental proposals
fn typescript_syntax(is_declaration_file: bool) -> Syntax {
    Syntax::Typescript(swc_ecma_parser::TsSyntax {
        tsx: false, // No JSX support in MVP
        decorators: false, // No experimental decorators
        dts: is_declaration_file, // Enable dts mode only for .d.ts files
        ..Default::default()
    })
}

/// Parse TypeScript source code into an AST module
///
/// Returns an error if:
/// - JSX syntax is encountered (not supported in MVP)
/// - Parse errors occur
pub fn parse_typescript(src: &str, source_map: &Lrc<SourceMap>, filename: &str) -> Result<Module> {
    // Check if this is a declaration file
    let is_dts = filename.ends_with(".d.ts");
    let syntax = typescript_syntax(is_dts);
    
    // Create SourceFile for the source code
    let source_file: Lrc<SourceFile> = source_map.new_source_file(
        FileName::Custom(filename.into()).into(),
        src.to_string(),
    );
    
    // Create StringInput from SourceFile
    let input = StringInput::from(&*source_file);
    
    // Create lexer with TypeScript syntax
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
            let error_debug = format!("{:?}", e.kind());
            
            // Check for JSX syntax errors
            if error_debug.contains("jsx") || 
               error_debug.contains("JSX") ||
               error_msg.contains("jsx") ||
               error_msg.contains("JSX") {
                anyhow::anyhow!("JSX syntax is not supported in MVP. Plain TypeScript only.")
            } else {
                anyhow::anyhow!("Parse error: {}", error_msg)
                    .context("Failed to parse TypeScript source")
            }
        })
}

#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
