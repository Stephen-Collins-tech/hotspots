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
    pub loc: usize,
}

/// Calculate lines of code (LOC) from source text
/// Counts physical lines (including blank lines and comments)
fn calculate_loc(source: &str) -> usize {
    if source.is_empty() {
        return 0;
    }
    // Count newlines + 1 for the last line (which may not end with \n)
    source.lines().count()
}

/// Calculate LOC from tree-sitter node
fn calculate_loc_from_node(node: &tree_sitter::Node) -> usize {
    let start_row = node.start_position().row;
    let end_row = node.end_position().row;
    // Rows are 0-indexed, so difference + 1 gives line count
    end_row.saturating_sub(start_row) + 1
}

/// Extract all metrics for a function
pub fn extract_metrics(function: &FunctionNode, cfg: &Cfg) -> RawMetrics {
    use crate::language::FunctionBody;

    match &function.body {
        FunctionBody::ECMAScript(body) => {
            // Calculate LOC from span (end_line - start_line + 1)
            let loc = function
                .span
                .end_line
                .saturating_sub(function.span.start_line)
                + 1;

            RawMetrics {
                cc: cyclomatic_complexity(cfg, body),
                nd: nesting_depth(body),
                fo: fan_out(body),
                ns: non_structured_exits(body),
                loc: loc as usize,
            }
        }
        FunctionBody::Go { .. } => {
            // Extract Go-specific metrics from tree-sitter AST
            extract_go_metrics(function, cfg)
        }
        FunctionBody::Java { .. } => {
            // Extract Java-specific metrics from tree-sitter AST
            extract_java_metrics(function, cfg)
        }
        FunctionBody::Python { .. } => {
            // Extract Python-specific metrics from tree-sitter AST
            extract_python_metrics(function, cfg)
        }
        FunctionBody::Rust { .. } => {
            // Extract Rust-specific metrics from syn AST
            extract_rust_metrics(function, cfg)
        }
    }
}

/// Calculate cyclomatic complexity from CFG alone
/// Used for languages where we don't yet have full AST metrics
fn calculate_cc_from_cfg(cfg: &Cfg) -> usize {
    // Base formula: CC = E - N + 2
    if cfg.edge_count() > 0 && cfg.node_count() > 2 {
        let e = cfg.edge_count();
        let n = cfg.node_count() - 2; // Exclude entry and exit
        if n > 0 {
            e.saturating_sub(n).saturating_add(2)
        } else {
            1
        }
    } else {
        1
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
    let mut visitor = NestingDepthVisitor {
        max_depth: 0,
        current_depth: 0,
    };
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
/// Count number of unique functions called by this function
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
    let has_final_return = body
        .stmts
        .last()
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

// ============================================================================
// Go Metrics Implementation
// ============================================================================

/// Extract metrics for Go functions using tree-sitter
fn extract_go_metrics(function: &FunctionNode, cfg: &Cfg) -> RawMetrics {
    let (_body_node_id, source) = function.body.as_go();

    // Re-parse the source to get the tree
    use tree_sitter::Parser;
    let mut parser = Parser::new();
    let language = tree_sitter_go::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Failed to set Go language");

    let tree = parser
        .parse(source, None)
        .expect("Failed to re-parse Go source");
    let root = tree.root_node();

    // Find the function node in the tree
    if let Some(func_node) = find_go_function_by_start(root, function.span.start) {
        // Find the block (function body)
        if let Some(body_node) = find_go_child_by_kind(func_node, "block") {
            // Calculate base CC from CFG
            let base_cc = calculate_cc_from_cfg(cfg);

            // Count additional CC contributors (switch cases, boolean operators)
            let extra_cc = go_count_cc_extras(&body_node, source);

            // Calculate other metrics from AST
            let nd = go_nesting_depth(&body_node);
            let fo = go_fan_out(&body_node, source);
            let ns = go_non_structured_exits(&body_node);

            return RawMetrics {
                cc: base_cc + extra_cc,
                nd,
                fo,
                ns,
                loc: calculate_loc_from_node(&func_node),
            };
        }
    }

    // Fallback: return minimal metrics
    RawMetrics {
        cc: 1,
        nd: 0,
        fo: 0,
        ns: 0,
        loc: 0,
    }
}

/// Find a Go function node by its start byte position
fn find_go_function_by_start(
    root: tree_sitter::Node,
    start_byte: usize,
) -> Option<tree_sitter::Node> {
    fn search_recursive(node: tree_sitter::Node, start: usize) -> Option<tree_sitter::Node> {
        if (node.kind() == "function_declaration" || node.kind() == "method_declaration")
            && node.start_byte() == start
        {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = search_recursive(child, start) {
                return Some(found);
            }
        }
        None
    }

    search_recursive(root, start_byte)
}

/// Find a child node by kind
#[allow(clippy::manual_find)]
fn find_go_child_by_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Calculate nesting depth for Go function
fn go_nesting_depth(body_node: &tree_sitter::Node) -> usize {
    fn calculate_depth(node: tree_sitter::Node, current_depth: usize, max_depth: &mut usize) {
        // Increment depth for control structures
        let new_depth = if matches!(
            node.kind(),
            "if_statement"
                | "for_statement"
                | "switch_statement"
                | "expression_switch_statement"
                | "type_switch_statement"
                | "select_statement"
        ) {
            let depth = current_depth + 1;
            if depth > *max_depth {
                *max_depth = depth;
            }
            depth
        } else {
            current_depth
        };

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            calculate_depth(child, new_depth, max_depth);
        }
    }

    let mut max_depth = 0;
    calculate_depth(*body_node, 0, &mut max_depth);
    max_depth
}

/// Calculate fan-out for Go function (function calls + go statements)
fn go_fan_out(body_node: &tree_sitter::Node, source: &str) -> usize {
    use std::collections::HashSet;

    fn count_calls(node: tree_sitter::Node, source: &str, calls: &mut HashSet<String>) {
        match node.kind() {
            "call_expression" => {
                // Extract function name
                if let Some(func_node) = find_go_child_by_kind(node, "identifier")
                    .or_else(|| find_go_child_by_kind(node, "selector_expression"))
                {
                    let func_text = &source[func_node.start_byte()..func_node.end_byte()];
                    calls.insert(func_text.to_string());
                }
            }
            "go_statement" => {
                // Go statements spawn goroutines, count as fan-out
                calls.insert(format!("<go@{}>", node.start_byte()));
            }
            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_calls(child, source, calls);
        }
    }

    let mut calls = HashSet::new();
    count_calls(*body_node, source, &mut calls);
    calls.len()
}

/// Calculate non-structured exits for Go function
fn go_non_structured_exits(body_node: &tree_sitter::Node) -> usize {
    fn count_exits(node: tree_sitter::Node, count: &mut usize) {
        match node.kind() {
            "return_statement" => *count += 1,
            "defer_statement" => *count += 1,
            "expression_statement" => {
                // Check if this is a panic call
                if let Some(call) = find_go_child_by_kind(node, "call_expression") {
                    if let Some(_ident) = find_go_child_by_kind(call, "identifier") {
                        // Would need source to check if it's "panic", but we can approximate
                        // by checking node structure
                        *count += 1;
                    }
                }
            }
            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_exits(child, count);
        }
    }

    let mut count = 0;
    count_exits(*body_node, &mut count);

    // Subtract 1 if the last statement is a return (final tail return)
    // This is an approximation - would need more sophisticated AST analysis
    if count > 0 {
        let mut cursor = body_node.walk();
        if let Some(last_child) = body_node.children(&mut cursor).last() {
            if last_child.kind() == "return_statement" {
                count = count.saturating_sub(1);
            }
        }
    }

    count
}

/// Count additional cyclomatic complexity contributors for Go
fn go_count_cc_extras(body_node: &tree_sitter::Node, source: &str) -> usize {
    fn count_extras(node: tree_sitter::Node, source: &str, count: &mut usize) {
        match node.kind() {
            // Count switch/select cases
            "expression_case" | "default_case" | "communication_case" | "type_case" => {
                *count += 1;
            }
            // Count boolean operators
            "binary_expression" => {
                // Check if it's && or ||
                let op_text = &source[node.start_byte()..node.end_byte()];
                if op_text.contains("&&") || op_text.contains("||") {
                    *count += 1;
                }
            }
            _ => {}
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_extras(child, source, count);
        }
    }

    let mut count = 0;
    count_extras(*body_node, source, &mut count);
    count
}

// Note: Go metrics tests are integrated with cfg_builder tests

// ============================================================================
// Java Metrics Implementation
// ============================================================================

/// Extract metrics for Java functions using tree-sitter
fn extract_java_metrics(function: &FunctionNode, cfg: &Cfg) -> RawMetrics {
    let (_body_node_id, source) = function.body.as_java();

    // Re-parse the source to get the tree
    use tree_sitter::Parser;
    let mut parser = Parser::new();
    let language = tree_sitter_java::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Failed to set Java language");

    let tree = parser
        .parse(source, None)
        .expect("Failed to re-parse Java source");
    let root = tree.root_node();

    // Find the function/method node in the tree
    if let Some(func_node) = find_java_function_by_start(root, function.span.start) {
        // Find the block (method body) or constructor_body
        if let Some(body_node) = find_java_child_by_kind(func_node, "block")
            .or_else(|| find_java_child_by_kind(func_node, "constructor_body"))
        {
            // Calculate base CC from CFG
            let base_cc = calculate_cc_from_cfg(cfg);

            // Count additional CC contributors (ternary, boolean operators, etc.)
            let extra_cc = java_count_cc_extras(&body_node, source);

            // Calculate other metrics from AST
            let nd = java_nesting_depth(&body_node);
            let fo = java_fan_out(&body_node, source);
            let ns = java_non_structured_exits(&body_node);

            return RawMetrics {
                cc: base_cc + extra_cc,
                nd,
                fo,
                ns,
                loc: calculate_loc_from_node(&func_node),
            };
        }
    }

    // Fallback: return minimal metrics
    RawMetrics {
        cc: 1,
        nd: 0,
        fo: 0,
        ns: 0,
        loc: 0,
    }
}

/// Find a Java function/method node by its start byte position
fn find_java_function_by_start(
    root: tree_sitter::Node,
    start_byte: usize,
) -> Option<tree_sitter::Node> {
    fn search_recursive(node: tree_sitter::Node, start: usize) -> Option<tree_sitter::Node> {
        if (node.kind() == "method_declaration" || node.kind() == "constructor_declaration")
            && node.start_byte() == start
        {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = search_recursive(child, start) {
                return Some(found);
            }
        }
        None
    }

    search_recursive(root, start_byte)
}

/// Find a child node by kind
#[allow(clippy::manual_find)]
fn find_java_child_by_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Calculate nesting depth for Java function
fn java_nesting_depth(body_node: &tree_sitter::Node) -> usize {
    fn calculate_depth(node: tree_sitter::Node, current_depth: usize, max_depth: &mut usize) {
        // Increment depth for control structures
        let new_depth = if matches!(
            node.kind(),
            "if_statement"
                | "while_statement"
                | "do_statement"
                | "for_statement"
                | "enhanced_for_statement"
                | "switch_statement"
                | "switch_expression"
                | "try_statement"
                | "synchronized_statement"
        ) {
            let depth = current_depth + 1;
            if depth > *max_depth {
                *max_depth = depth;
            }
            depth
        } else {
            current_depth
        };

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            calculate_depth(child, new_depth, max_depth);
        }
    }

    let mut max_depth = 0;
    calculate_depth(*body_node, 0, &mut max_depth);
    max_depth
}

/// Count method calls (fan-out) in Java
fn java_fan_out(body_node: &tree_sitter::Node, source: &str) -> usize {
    fn count_calls(
        node: tree_sitter::Node,
        source: &str,
        calls: &mut std::collections::HashSet<String>,
    ) {
        // Java uses "method_invocation" node for method calls
        if node.kind() == "method_invocation" {
            // Extract the method name
            let method_text = &source[node.start_byte()..node.end_byte()];
            calls.insert(method_text.to_string());
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_calls(child, source, calls);
        }
    }

    let mut calls = std::collections::HashSet::new();
    count_calls(*body_node, source, &mut calls);
    calls.len()
}

/// Count non-structured exits in Java (return, throw, break, continue)
fn java_non_structured_exits(body_node: &tree_sitter::Node) -> usize {
    fn count_exits(node: tree_sitter::Node, count: &mut usize) {
        match node.kind() {
            "return_statement" | "throw_statement" | "break_statement" | "continue_statement" => {
                *count += 1;
            }
            _ => {}
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_exits(child, count);
        }
    }

    let mut count = 0;
    count_exits(*body_node, &mut count);
    count
}

/// Count additional CC contributors in Java
/// (ternary expressions, boolean operators)
fn java_count_cc_extras(body_node: &tree_sitter::Node, source: &str) -> usize {
    fn count_extras(node: tree_sitter::Node, source: &str, count: &mut usize) {
        match node.kind() {
            // Ternary expressions (conditional_expression) add to CC
            "ternary_expression" => {
                *count += 1;
            }
            // Binary expressions with && or || add to CC
            "binary_expression" => {
                // Check if operator is && or ||
                let text = &source[node.start_byte()..node.end_byte()];
                if text.contains("&&") || text.contains("||") {
                    *count += 1;
                }
            }
            _ => {}
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_extras(child, source, count);
        }
    }

    let mut count = 0;
    count_extras(*body_node, source, &mut count);
    count
}

// ============================================================================
// Python Metrics Implementation
// ============================================================================

/// Extract metrics for Python functions using tree-sitter
fn extract_python_metrics(function: &FunctionNode, cfg: &Cfg) -> RawMetrics {
    let (_body_node_id, source) = function.body.as_python();

    // Re-parse the source to get the tree
    use tree_sitter::Parser;
    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Failed to set Python language");

    let tree = parser
        .parse(source, None)
        .expect("Failed to re-parse Python source");
    let root = tree.root_node();

    // Find the function node in the tree
    if let Some(func_node) = find_python_function_by_start(root, function.span.start) {
        // Find the block (function body)
        if let Some(body_node) = find_python_child_by_kind(func_node, "block") {
            // Calculate base CC from CFG
            let base_cc = calculate_cc_from_cfg(cfg);

            // Count additional CC contributors (comprehensions, boolean operators, etc.)
            let extra_cc = python_count_cc_extras(&body_node, source);

            // Calculate other metrics from AST
            let nd = python_nesting_depth(&body_node);
            let fo = python_fan_out(&body_node, source);
            let ns = python_non_structured_exits(&body_node);

            return RawMetrics {
                cc: base_cc + extra_cc,
                nd,
                fo,
                ns,
                loc: calculate_loc_from_node(&func_node),
            };
        }
    }

    // Fallback: return minimal metrics
    RawMetrics {
        cc: 1,
        nd: 0,
        fo: 0,
        ns: 0,
        loc: 0,
    }
}

/// Find a Python function node by its start byte position
fn find_python_function_by_start(
    root: tree_sitter::Node,
    start_byte: usize,
) -> Option<tree_sitter::Node> {
    fn search_recursive(node: tree_sitter::Node, start: usize) -> Option<tree_sitter::Node> {
        if (node.kind() == "function_definition" || node.kind() == "async_function_definition")
            && node.start_byte() == start
        {
            return Some(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = search_recursive(child, start) {
                return Some(found);
            }
        }
        None
    }

    search_recursive(root, start_byte)
}

/// Find a child node by kind
#[allow(clippy::manual_find)]
fn find_python_child_by_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Calculate nesting depth for Python function
fn python_nesting_depth(body_node: &tree_sitter::Node) -> usize {
    fn calculate_depth(node: tree_sitter::Node, current_depth: usize, max_depth: &mut usize) {
        // Increment depth for control structures
        let new_depth = if matches!(
            node.kind(),
            "if_statement"
                | "while_statement"
                | "for_statement"
                | "try_statement"
                | "with_statement"
                | "match_statement"
        ) {
            let depth = current_depth + 1;
            if depth > *max_depth {
                *max_depth = depth;
            }
            depth
        } else {
            current_depth
        };

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            calculate_depth(child, new_depth, max_depth);
        }
    }

    let mut max_depth = 0;
    calculate_depth(*body_node, 0, &mut max_depth);
    max_depth
}

/// Count function calls (fan-out) in Python
fn python_fan_out(body_node: &tree_sitter::Node, source: &str) -> usize {
    fn count_calls(
        node: tree_sitter::Node,
        source: &str,
        calls: &mut std::collections::HashSet<String>,
    ) {
        // Python uses "call" node for function calls
        if node.kind() == "call" {
            // Try to extract the function name
            let mut cursor = node.walk();
            if let Some(func_node) = node.children(&mut cursor).next() {
                let func_text = &source[func_node.start_byte()..func_node.end_byte()];
                calls.insert(func_text.to_string());
            };
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_calls(child, source, calls);
        }
    }

    let mut calls = std::collections::HashSet::new();
    count_calls(*body_node, source, &mut calls);
    calls.len()
}

/// Count non-structured exits in Python (return, raise, break, continue)
fn python_non_structured_exits(body_node: &tree_sitter::Node) -> usize {
    fn count_exits(node: tree_sitter::Node, count: &mut usize) {
        match node.kind() {
            "return_statement" | "raise_statement" | "break_statement" | "continue_statement" => {
                *count += 1;
            }
            _ => {}
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_exits(child, count);
        }
    }

    let mut count = 0;
    count_exits(*body_node, &mut count);
    count
}

/// Count additional CC contributors in Python
/// (comprehensions with if-filters, boolean operators, ternary expressions)
fn python_count_cc_extras(body_node: &tree_sitter::Node, _source: &str) -> usize {
    fn count_extras(node: tree_sitter::Node, count: &mut usize) {
        match node.kind() {
            // Boolean operators (and, or) add to CC
            "boolean_operator" => {
                *count += 1;
            }
            // Ternary expressions add to CC
            "conditional_expression" => {
                *count += 1;
            }
            // Comprehensions with if-filters add to CC
            "list_comprehension"
            | "dictionary_comprehension"
            | "set_comprehension"
            | "generator_expression" => {
                // Check if it has an if_clause child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "if_clause" {
                        *count += 1;
                        break;
                    }
                }
            }
            _ => {}
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            count_extras(child, count);
        }
    }

    let mut count = 0;
    count_extras(*body_node, &mut count);
    count
}

// Note: Python metrics tests are integrated with cfg_builder tests

// ========================================
// Rust Metrics Extraction
// ========================================

/// Extract metrics for a Rust function
fn extract_rust_metrics(function: &FunctionNode, cfg: &Cfg) -> RawMetrics {
    let source = function.body.as_rust();

    // Parse the function source
    let item_fn: syn::ItemFn = match syn::parse_str(source) {
        Ok(func) => func,
        Err(_) => {
            // Fallback on parse error
            return RawMetrics {
                cc: calculate_cc_from_cfg(cfg),
                nd: 0,
                fo: 0,
                ns: 0,
                loc: 0,
            };
        }
    };

    let base_cc = calculate_cc_from_cfg(cfg);
    let extra_cc = rust_count_cc_extras(&item_fn.block);
    let nd = rust_nesting_depth(&item_fn.block);
    let fo = rust_fan_out(&item_fn.block);
    let ns = rust_non_structured_exits(&item_fn.block);

    RawMetrics {
        cc: base_cc + extra_cc,
        nd,
        fo,
        ns,
        loc: calculate_loc(source),
    }
}

/// Calculate nesting depth for Rust function
fn rust_nesting_depth(block: &syn::Block) -> usize {
    use syn::{Expr, Stmt};

    fn calculate_depth(stmts: &[Stmt], current_depth: usize, max_depth: &mut usize) {
        for stmt in stmts {
            match stmt {
                Stmt::Expr(expr, _) => expr_depth(expr, current_depth, max_depth),
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        expr_depth(&init.expr, current_depth, max_depth);
                    }
                }
                _ => {}
            }
        }
    }

    fn expr_depth(expr: &Expr, current_depth: usize, max_depth: &mut usize) {
        let new_depth = match expr {
            Expr::If(_) | Expr::Match(_) | Expr::Loop(_) | Expr::While(_) | Expr::ForLoop(_) => {
                let depth = current_depth + 1;
                if depth > *max_depth {
                    *max_depth = depth;
                }
                depth
            }
            _ => current_depth,
        };

        // Recurse into sub-expressions
        match expr {
            Expr::If(expr_if) => {
                calculate_depth(&expr_if.then_branch.stmts, new_depth, max_depth);
                if let Some((_, else_expr)) = &expr_if.else_branch {
                    expr_depth(else_expr, new_depth, max_depth);
                }
            }
            Expr::Match(expr_match) => {
                for arm in &expr_match.arms {
                    expr_depth(&arm.body, new_depth, max_depth);
                }
            }
            Expr::Loop(expr_loop) => {
                calculate_depth(&expr_loop.body.stmts, new_depth, max_depth);
            }
            Expr::While(expr_while) => {
                calculate_depth(&expr_while.body.stmts, new_depth, max_depth);
            }
            Expr::ForLoop(expr_for) => {
                calculate_depth(&expr_for.body.stmts, new_depth, max_depth);
            }
            Expr::Block(expr_block) => {
                calculate_depth(&expr_block.block.stmts, new_depth, max_depth);
            }
            _ => {}
        }
    }

    let mut max_depth = 0;
    calculate_depth(&block.stmts, 0, &mut max_depth);
    max_depth
}

/// Calculate fan-out for Rust function
fn rust_fan_out(block: &syn::Block) -> usize {
    use std::collections::HashSet;
    use syn::{Expr, ExprCall, ExprMethodCall, Stmt};

    fn count_calls(stmts: &[Stmt], calls: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Expr(expr, _) => expr_calls(expr, calls),
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        expr_calls(&init.expr, calls);
                    }
                }
                Stmt::Macro(stmt_macro) => {
                    // Count macro invocations
                    let macro_name = stmt_macro
                        .mac
                        .path
                        .segments
                        .last()
                        .map(|seg| seg.ident.to_string())
                        .unwrap_or_else(|| "macro".to_string());
                    calls.insert(macro_name);
                }
                _ => {}
            }
        }
    }

    fn expr_calls(expr: &Expr, calls: &mut HashSet<String>) {
        match expr {
            Expr::Call(ExprCall { func, .. }) => {
                // Extract function name from path
                if let Expr::Path(expr_path) = &**func {
                    let func_name = expr_path
                        .path
                        .segments
                        .last()
                        .map(|seg| seg.ident.to_string())
                        .unwrap_or_else(|| "fn".to_string());
                    calls.insert(func_name);
                }
            }
            Expr::MethodCall(ExprMethodCall { method, .. }) => {
                calls.insert(method.to_string());
            }
            Expr::Macro(expr_macro) => {
                let macro_name = expr_macro
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|seg| seg.ident.to_string())
                    .unwrap_or_else(|| "macro".to_string());
                calls.insert(macro_name);
            }
            Expr::If(expr_if) => {
                expr_calls(&expr_if.cond, calls);
                count_calls(&expr_if.then_branch.stmts, calls);
                if let Some((_, else_expr)) = &expr_if.else_branch {
                    expr_calls(else_expr, calls);
                }
            }
            Expr::Match(expr_match) => {
                expr_calls(&expr_match.expr, calls);
                for arm in &expr_match.arms {
                    expr_calls(&arm.body, calls);
                }
            }
            Expr::Loop(expr_loop) => {
                count_calls(&expr_loop.body.stmts, calls);
            }
            Expr::While(expr_while) => {
                expr_calls(&expr_while.cond, calls);
                count_calls(&expr_while.body.stmts, calls);
            }
            Expr::ForLoop(expr_for) => {
                expr_calls(&expr_for.expr, calls);
                count_calls(&expr_for.body.stmts, calls);
            }
            Expr::Block(expr_block) => {
                count_calls(&expr_block.block.stmts, calls);
            }
            _ => {}
        }
    }

    let mut calls = HashSet::new();
    count_calls(&block.stmts, &mut calls);
    calls.len()
}

/// Calculate non-structured exits for Rust function
fn rust_non_structured_exits(block: &syn::Block) -> usize {
    use syn::{Expr, ExprMethodCall, Stmt};

    fn count_exits(stmts: &[Stmt], count: &mut usize, is_tail: bool) {
        for (i, stmt) in stmts.iter().enumerate() {
            let is_last = i == stmts.len() - 1;
            match stmt {
                Stmt::Expr(expr, _) => {
                    expr_exits(expr, count, is_tail && is_last);
                }
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        expr_exits(&init.expr, count, false);
                    }
                }
                _ => {}
            }
        }
    }

    fn expr_exits(expr: &Expr, count: &mut usize, is_tail: bool) {
        match expr {
            Expr::Return(_) => {
                // Don't count final tail return
                if !is_tail {
                    *count += 1;
                }
            }
            Expr::Try(_) => {
                // ? operator counts as early return
                *count += 1;
            }
            Expr::MethodCall(ExprMethodCall { method, .. }) => {
                // unwrap, expect, panic count as non-structured exits
                let method_name = method.to_string();
                if matches!(
                    method_name.as_str(),
                    "unwrap" | "expect" | "unwrap_or_else" | "unwrap_or"
                ) {
                    *count += 1;
                }
            }
            Expr::Macro(expr_macro) => {
                // panic!, unreachable!, unimplemented! count as exits
                if let Some(segment) = expr_macro.mac.path.segments.last() {
                    let macro_name = segment.ident.to_string();
                    if matches!(
                        macro_name.as_str(),
                        "panic" | "unreachable" | "unimplemented" | "todo"
                    ) {
                        *count += 1;
                    }
                }
            }
            Expr::If(expr_if) => {
                expr_exits(&expr_if.cond, count, false);
                count_exits(&expr_if.then_branch.stmts, count, false);
                if let Some((_, else_expr)) = &expr_if.else_branch {
                    expr_exits(else_expr, count, false);
                }
            }
            Expr::Match(expr_match) => {
                expr_exits(&expr_match.expr, count, false);
                for arm in &expr_match.arms {
                    expr_exits(&arm.body, count, false);
                }
            }
            Expr::Loop(expr_loop) => {
                count_exits(&expr_loop.body.stmts, count, false);
            }
            Expr::While(expr_while) => {
                expr_exits(&expr_while.cond, count, false);
                count_exits(&expr_while.body.stmts, count, false);
            }
            Expr::ForLoop(expr_for) => {
                expr_exits(&expr_for.expr, count, false);
                count_exits(&expr_for.body.stmts, count, false);
            }
            Expr::Block(expr_block) => {
                count_exits(&expr_block.block.stmts, count, is_tail);
            }
            _ => {}
        }
    }

    let mut count = 0;
    count_exits(&block.stmts, &mut count, true);
    count
}

/// Count CC extras for Rust (match arms, boolean operators)
fn rust_count_cc_extras(block: &syn::Block) -> usize {
    use syn::{BinOp, Expr, Stmt};

    fn count_extras(stmts: &[Stmt], count: &mut usize) {
        for stmt in stmts {
            match stmt {
                Stmt::Expr(expr, _) => expr_extras(expr, count),
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        expr_extras(&init.expr, count);
                    }
                }
                _ => {}
            }
        }
    }

    fn expr_extras(expr: &Expr, count: &mut usize) {
        match expr {
            Expr::Match(expr_match) => {
                // Each match arm is a decision point
                *count += expr_match.arms.len();
                expr_extras(&expr_match.expr, count);
                for arm in &expr_match.arms {
                    expr_extras(&arm.body, count);
                }
            }
            Expr::Binary(expr_binary) => {
                // Boolean operators
                if matches!(expr_binary.op, BinOp::And(_) | BinOp::Or(_)) {
                    *count += 1;
                }
                expr_extras(&expr_binary.left, count);
                expr_extras(&expr_binary.right, count);
            }
            Expr::If(expr_if) => {
                expr_extras(&expr_if.cond, count);
                count_extras(&expr_if.then_branch.stmts, count);
                if let Some((_, else_expr)) = &expr_if.else_branch {
                    expr_extras(else_expr, count);
                }
            }
            Expr::Loop(expr_loop) => {
                count_extras(&expr_loop.body.stmts, count);
            }
            Expr::While(expr_while) => {
                expr_extras(&expr_while.cond, count);
                count_extras(&expr_while.body.stmts, count);
            }
            Expr::ForLoop(expr_for) => {
                expr_extras(&expr_for.expr, count);
                count_extras(&expr_for.body.stmts, count);
            }
            Expr::Block(expr_block) => {
                count_extras(&expr_block.block.stmts, count);
            }
            _ => {}
        }
    }

    let mut count = 0;
    count_extras(&block.stmts, &mut count);
    count
}

// Note: Rust metrics tests are integrated with parser/cfg_builder tests
