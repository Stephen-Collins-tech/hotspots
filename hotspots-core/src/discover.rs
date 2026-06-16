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
use crate::language::{span::span_with_location, FunctionBody};
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
        source_map,
        pending_name: None,
    };

    module.visit_with(&mut collector);

    // Sort by span start for deterministic ordering
    collector.functions.sort_by_key(|f| f.span.start);

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
            func.suppression_reason =
                crate::suppression::extract_suppression(source, func.span, source_map);
            func
        })
        .collect()
}

/// Visitor to collect function nodes from the AST
struct FunctionCollector<'a> {
    file_index: usize,
    functions: Vec<FunctionNode>,
    local_index: usize,
    source_map: &'a swc_common::SourceMap,
    /// Name of the variable a function/arrow expression is being assigned to
    /// (e.g. `const Foo = () => {...}`), set while visiting the declarator's
    /// init expression so the function picks it up instead of `<anonymous>`.
    pending_name: Option<String>,
}

impl<'a> Visit for FunctionCollector<'a> {
    fn visit_var_declarator(&mut self, decl: &VarDeclarator) {
        if let (Pat::Ident(ident), Some(init)) = (&decl.name, &decl.init) {
            if matches!(&**init, Expr::Fn(_) | Expr::Arrow(_)) {
                self.pending_name = Some(ident.id.sym.to_string());
            }
        }
        decl.visit_children_with(self);
        self.pending_name = None;
    }

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
                span: span_with_location(decl.function.span, self.source_map),
                body: FunctionBody::ecmascript(body),
                suppression_reason: None,
            });
            self.local_index += 1;
        }

        // Continue visiting children
        decl.visit_children_with(self);
    }

    fn visit_fn_expr(&mut self, expr: &FnExpr) {
        // Extract function name; fall back to the variable it's assigned to
        // (e.g. `const Foo = function() {...}`)
        let name = expr
            .ident
            .as_ref()
            .map(|id| id.sym.to_string())
            .or_else(|| self.pending_name.take());

        // Extract body
        let body = expr.function.body.clone();

        if let Some(body) = body {
            self.functions.push(FunctionNode {
                id: FunctionId {
                    file_index: self.file_index,
                    local_index: self.local_index,
                },
                name,
                span: span_with_location(expr.function.span, self.source_map),
                body: FunctionBody::ecmascript(body),
                suppression_reason: None,
            });
            self.local_index += 1;
        }

        // Continue visiting children
        expr.visit_children_with(self);
    }

    fn visit_arrow_expr(&mut self, arrow: &ArrowExpr) {
        // Use the variable it's assigned to (e.g. `const Foo = () => {...}`),
        // falling back to <anonymous>@file:line in the name extraction
        let name = self.pending_name.take();

        match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(ref body) => {
                self.functions.push(FunctionNode {
                    id: FunctionId {
                        file_index: self.file_index,
                        local_index: self.local_index,
                    },
                    name,
                    span: span_with_location(arrow.span, self.source_map),
                    body: FunctionBody::ecmascript(body.clone()),
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
                    span: span_with_location(arrow.span, self.source_map),
                    body: FunctionBody::ecmascript(body),
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
                span: span_with_location(method.span, self.source_map),
                body: FunctionBody::ecmascript(body),
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
                span: span_with_location(method.function.span, self.source_map),
                body: FunctionBody::ecmascript(body),
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
