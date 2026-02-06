//! Metric extraction from AST and CFG
//!
//! Global invariants enforced:
//! - Formatting, comments, and whitespace must not affect results
//! - Deterministic metric calculation

use crate::ast::FunctionNode;
use crate::cfg::Cfg;
use swc_ecma_ast::*;
use swc_ecma_visit::{Visit, VisitWith};

/// Raw metrics for a function
#[derive(Debug, Clone)]
pub struct RawMetrics {
    pub cc: usize,
    pub nd: usize,
    pub fo: usize,
    pub ns: usize,
}

/// Extract all metrics for a function
pub fn extract_metrics(function: &FunctionNode, cfg: &Cfg) -> RawMetrics {
    RawMetrics {
        cc: cyclomatic_complexity(cfg, &function.body),
        nd: nesting_depth(&function.body),
        fo: fan_out(&function.body),
        ns: non_structured_exits(&function.body),
    }
}

/// Calculate Cyclomatic Complexity: CC = E - N + 2
///
/// Additional increments:
/// - Boolean short-circuit operators (&&, ||)
/// - Each switch case
/// - Each catch clause
fn cyclomatic_complexity(cfg: &Cfg, body: &BlockStmt) -> usize {
    // Base formula: CC = E - N + 2
    let base_cc = if cfg.edge_count() > 0 && cfg.node_count() > 2 {
        // Exclude entry and exit nodes for calculation
        // E = number of edges
        // N = number of nodes (excluding entry/exit which are structural)
        let e = cfg.edge_count();
        let n = cfg.node_count() - 2; // Exclude entry and exit
        if n > 0 {
            e.saturating_sub(n).saturating_add(2)
        } else {
            1 // Minimum CC for any function
        }
    } else {
        1 // Empty function has CC = 1
    };
    
    // Increment for boolean short-circuit operators
    let mut short_circuit_count = 0;
    let mut visitor = ShortCircuitVisitor {
        count: &mut short_circuit_count,
    };
    body.visit_with(&mut visitor);
    
    // Increment for switch cases
    let switch_case_count = count_switch_cases(body);
    
    // Increment for catch clauses
    let catch_count = count_catch_clauses(body);
    
    base_cc + short_circuit_count + switch_case_count + catch_count
}

/// Visitor to count boolean short-circuit operators
struct ShortCircuitVisitor<'a> {
    count: &'a mut usize,
}

impl Visit for ShortCircuitVisitor<'_> {
    fn visit_bin_expr(&mut self, bin_expr: &BinExpr) {
        match bin_expr.op {
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr => {
                *self.count += 1;
            }
            _ => {}
        }
        bin_expr.visit_children_with(self);
    }
}

/// Count switch cases in the AST
fn count_switch_cases(body: &BlockStmt) -> usize {
    let mut count = 0;
    let mut visitor = SwitchCaseCounter { count: &mut count };
    body.visit_with(&mut visitor);
    count
}

struct SwitchCaseCounter<'a> {
    count: &'a mut usize,
}

impl Visit for SwitchCaseCounter<'_> {
    fn visit_switch_stmt(&mut self, switch_stmt: &SwitchStmt) {
        // Count each case in the switch
        *self.count += switch_stmt.cases.len();
        switch_stmt.visit_children_with(self);
    }
}

/// Count catch clauses in the AST
fn count_catch_clauses(body: &BlockStmt) -> usize {
    let mut count = 0;
    let mut visitor = CatchCounter { count: &mut count };
    body.visit_with(&mut visitor);
    count
}

struct CatchCounter<'a> {
    count: &'a mut usize,
}

impl Visit for CatchCounter<'_> {
    fn visit_try_stmt(&mut self, try_stmt: &TryStmt) {
        // Count catch clause if present
        if try_stmt.handler.is_some() {
            *self.count += 1;
        }
        try_stmt.visit_children_with(self);
    }
}

/// Calculate Nesting Depth (ND)
///
/// Walk AST and count maximum depth of control constructs:
/// - if, loop, switch, try
fn nesting_depth(body: &BlockStmt) -> usize {
    let mut visitor = NestingDepthVisitor { max_depth: 0, current_depth: 0 };
    body.visit_with(&mut visitor);
    visitor.max_depth
}

struct NestingDepthVisitor {
    max_depth: usize,
    current_depth: usize,
}

impl Visit for NestingDepthVisitor {
    fn visit_if_stmt(&mut self, if_stmt: &IfStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        if_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_while_stmt(&mut self, while_stmt: &WhileStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        while_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_do_while_stmt(&mut self, do_while_stmt: &DoWhileStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        do_while_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_for_stmt(&mut self, for_stmt: &ForStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        for_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_for_in_stmt(&mut self, for_in_stmt: &ForInStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        for_in_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_for_of_stmt(&mut self, for_of_stmt: &ForOfStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        for_of_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_switch_stmt(&mut self, switch_stmt: &SwitchStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        switch_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }

    fn visit_try_stmt(&mut self, try_stmt: &TryStmt) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
        try_stmt.visit_children_with(self);
        self.current_depth -= 1;
    }
}

/// Calculate Fan-Out (FO)
///
/// Collect call expressions and extract callee identifiers.
/// For chained calls, count each call expression independently.
/// Deduplicate by symbol name.
fn fan_out(body: &BlockStmt) -> usize {
    let mut visitor = FanOutVisitor {
        calls: std::collections::HashSet::new(),
    };
    body.visit_with(&mut visitor);
    visitor.calls.len()
}

struct FanOutVisitor {
    calls: std::collections::HashSet<String>,
}

impl Visit for FanOutVisitor {
    fn visit_call_expr(&mut self, call_expr: &CallExpr) {
        // Extract callee representation
        let callee_str = callee_to_string(&call_expr.callee);
        if !callee_str.is_empty() && callee_str != "<computed>" {
            self.calls.insert(callee_str);
        }
        
        // Continue visiting children (to catch chained calls)
        call_expr.visit_children_with(self);
    }
}

/// Convert a callee expression to string representation
fn callee_to_string(callee: &Callee) -> String {
    match callee {
        Callee::Expr(expr) => expr_to_callee_string(expr),
        Callee::Super(_) => "super".to_string(),
        Callee::Import(_) => "<computed>".to_string(),
    }
}

/// Extract string representation from expression for callee
/// 
/// For chained calls like foo().bar().baz(), we extract:
/// - foo (when visiting the inner CallExpr)
/// - foo().bar (when visiting the middle CallExpr with MemberExpr callee)
/// - foo().bar().baz (when visiting the outer CallExpr with MemberExpr callee)
fn expr_to_callee_string(expr: &Expr) -> String {
    match expr {
        Expr::Ident(ident) => ident.sym.to_string(),
        Expr::Member(member) => {
            // For member expressions like obj.method, represent as obj.method
            // The obj might be a call (for chained calls)
            let obj_str = match &*member.obj {
                Expr::Ident(id) => id.sym.to_string(),
                Expr::Call(call) => {
                    // Chained call - extract the callee of the inner call
                    // This gives us the full chain like "foo().bar" when processing "foo().bar().baz"
                    match &call.callee {
                        Callee::Expr(callee_expr) => expr_to_callee_string(callee_expr),
                        _ => "<computed>".to_string(),
                    }
                }
                Expr::Member(member_obj) => {
                    // Nested member expression - recursively build the chain
                    expr_to_callee_string(&member_obj.obj)
                }
                _ => "<computed>".to_string(),
            };
            
            let prop_str = match &member.prop {
                MemberProp::Ident(id) => id.sym.to_string(),
                MemberProp::PrivateName(name) => name.name.to_string(),
                MemberProp::Computed(_) => "<computed>".to_string(),
            };
            
            if obj_str == "<computed>" || prop_str == "<computed>" {
                "<computed>".to_string()
            } else {
                format!("{}.{}", obj_str, prop_str)
            }
        }
        Expr::Call(_) => "<computed>".to_string(), // Should not happen - CallExpr callee should be MemberExpr or Ident
        _ => "<computed>".to_string(),
    }
}

/// Calculate Non-Structured Exits (NS)
///
/// Count:
/// - Early return statements (excluding final tail return)
/// - Break statements
/// - Continue statements
/// - Throw statements
fn non_structured_exits(body: &BlockStmt) -> usize {
    let mut visitor = NonStructuredExitVisitor {
        count: 0,
        return_count: 0,
    };
    body.visit_with(&mut visitor);
    
    // Check if last statement is a return (final tail return)
    let has_final_return = body.stmts.last()
        .map(|s| matches!(s, Stmt::Return(_)))
        .unwrap_or(false);
    
    // Exclude final return if present
    if has_final_return && visitor.return_count > 0 {
        visitor.count -= 1;
    }
    
    visitor.count
}

struct NonStructuredExitVisitor {
    count: usize,
    return_count: usize,
}

impl Visit for NonStructuredExitVisitor {
    fn visit_return_stmt(&mut self, _return_stmt: &ReturnStmt) {
        self.count += 1;
        self.return_count += 1;
    }

    fn visit_break_stmt(&mut self, _break_stmt: &BreakStmt) {
        self.count += 1;
    }

    fn visit_continue_stmt(&mut self, _continue_stmt: &ContinueStmt) {
        self.count += 1;
    }

    fn visit_throw_stmt(&mut self, _throw_stmt: &ThrowStmt) {
        self.count += 1;
    }
}
