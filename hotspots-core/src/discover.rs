//! Function discovery from AST
//!
//! Global invariants enforced:
//! - Deterministic traversal order by (file, span.start)
//! - Per-function analysis only
//!
//! Supported constructs:
//! - Function declarations (`FnDecl`)
//! - Function expressions (`FnExpr`)
//! - Arrow functions (`ArrowExpr`)
//! - Class methods (`ClassMethod`)
//! - Object literal methods (`MethodProp`)
//!
//! Ignored constructs (automatically excluded as they have no function bodies):
//! - Interfaces
//! - Type aliases
//! - Overload signatures without bodies (filtered by `if let Some(body)`)
//! - Ambient declarations

use crate::ast::{FunctionId, FunctionNode};
use swc_ecma_ast::*;
use swc_ecma_visit::{Visit, VisitWith};

/// Collect all functions from a TypeScript module
///
/// Returns functions sorted deterministically by span start position.
pub fn discover_functions(
    module: &Module,
    file_index: usize,
    source: &str,
    source_map: &swc_common::SourceMap,
) -> Vec<FunctionNode> {
    let mut collector = FunctionCollector {
        file_index,
        functions: Vec::new(),
        local_index: 0,
    };

    module.visit_with(&mut collector);

    // Sort by span start for deterministic ordering
    collector.functions.sort_by_key(|f| f.span.lo);

    // Assign IDs and extract suppressions based on sorted order
    collector
        .functions
        .into_iter()
        .enumerate()
        .map(|(idx, mut func)| {
            func.id = FunctionId {
                file_index,
                local_index: idx,
            };
            // Extract suppression comment for this function
            func.suppression_reason = crate::suppression::extract_suppression(
                source,
                func.span,
                source_map,
            );
            func
        })
        .collect()
}

/// Visitor to collect function nodes from the AST
struct FunctionCollector {
    file_index: usize,
    functions: Vec<FunctionNode>,
    local_index: usize,
}

impl Visit for FunctionCollector {
    fn visit_fn_decl(&mut self, decl: &FnDecl) {
        // Extract function name from declaration
        let name = Some(decl.ident.sym.to_string());

        // Extract body
        let body = decl.function.body.clone();
        
        if let Some(body) = body {
            self.functions.push(FunctionNode {
                id: FunctionId {
                    file_index: self.file_index,
                    local_index: self.local_index,
                },
                name,
                span: decl.function.span,
                body,
                suppression_reason: None,
            });
            self.local_index += 1;
        }
        
        // Continue visiting children
        decl.visit_children_with(self);
    }

    fn visit_fn_expr(&mut self, expr: &FnExpr) {
        // Extract function name (may be None for anonymous)
        let name = expr.ident.as_ref().map(|id| id.sym.to_string());

        // Extract body
        let body = expr.function.body.clone();
        
        if let Some(body) = body {
            self.functions.push(FunctionNode {
                id: FunctionId {
                    file_index: self.file_index,
                    local_index: self.local_index,
                },
                name,
                span: expr.function.span,
                body,
                suppression_reason: None,
            });
            self.local_index += 1;
        }
        
        // Continue visiting children
        expr.visit_children_with(self);
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        // Generate synthetic name for anonymous arrow function
        let name = None; // Will be set to <anonymous>@file:line in the name extraction
        
        match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(ref body) => {
                self.functions.push(FunctionNode {
                    id: FunctionId {
                        file_index: self.file_index,
                        local_index: self.local_index,
                    },
                    name,
                    span: arrow.span,
                    body: body.clone(),
                    suppression_reason: None,
                });
                self.local_index += 1;
            }
            BlockStmtOrExpr::Expr(ref expr) => {
                // Arrow function with expression body - treat as implicit return
                // Create a synthetic block with a single return statement
                let return_stmt = Stmt::Return(ReturnStmt {
                    span: arrow.span,
                    arg: Some(expr.clone()),
                });
                let body = BlockStmt {
                    span: arrow.span,
                    ctxt: arrow.ctxt,
                    stmts: vec![return_stmt],
                };
                
                self.functions.push(FunctionNode {
                    id: FunctionId {
                        file_index: self.file_index,
                        local_index: self.local_index,
                    },
                    name,
                    span: arrow.span,
                    body,
                    suppression_reason: None,
                });
                self.local_index += 1;
            }
        }
        
        // Continue visiting children
        arrow.visit_children_with(self);
    }

    fn visit_class_method(&mut self, method: &ClassMethod) {
        let name = match &method.key {
            PropName::Ident(ident) => Some(ident.sym.to_string()),
            PropName::Str(str_lit) => {
                // Wtf8Atom to String via to_atom_lossy (borrows when possible)
                Some(str_lit.value.to_atom_lossy().to_string())
            }
            PropName::Num(num) => Some(num.to_string()),
            _ => None,
        };

        let body = method.function.body.clone();
        
        if let Some(body) = body {
            self.functions.push(FunctionNode {
                id: FunctionId {
                    file_index: self.file_index,
                    local_index: self.local_index,
                },
                name,
                span: method.span,
                body,
                suppression_reason: None,
            });
            self.local_index += 1;
        }
        
        // Continue visiting children
        method.visit_children_with(self);
    }

    fn visit_method_prop(&mut self, method: &MethodProp) {
        let name = match &method.key {
            PropName::Ident(ident) => Some(ident.sym.to_string()),
            PropName::Str(str_lit) => {
                // Wtf8Atom to String via to_atom_lossy (borrows when possible)
                Some(str_lit.value.to_atom_lossy().to_string())
            }
            PropName::Num(num) => Some(num.to_string()),
            _ => None,
        };

        let body = method.function.body.clone();
        
        if let Some(body) = body {
            self.functions.push(FunctionNode {
                id: FunctionId {
                    file_index: self.file_index,
                    local_index: self.local_index,
                },
                name,
                span: method.function.span,
                body,
                suppression_reason: None,
            });
            self.local_index += 1;
        }
        
        // Continue visiting children
        method.visit_children_with(self);
    }
}

#[cfg(test)]
#[path = "discover/tests.rs"]
mod tests;
