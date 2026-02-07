//! Python CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use tree_sitter::{Node, Parser};

/// Python CFG builder
///
/// Builds control flow graphs from Python function bodies parsed with tree-sitter.
pub struct PythonCfgBuilder;

impl CfgBuilder for PythonCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_python();

        // Re-parse the source to get the tree
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser.set_language(&language.into()).expect("Failed to set Python language");

        let tree = parser.parse(source, None).expect("Failed to re-parse Python source");
        let root = tree.root_node();

        // Find the function node in the tree
        if let Some(func_node) = find_function_by_start(root, function.span.start) {
            // Find the block (function body)
            if let Some(body_node) = find_child_by_kind(func_node, "block") {
                let mut builder = PythonCfgBuilderState::new();
                builder.build_from_block(&body_node, source);
                return builder.cfg;
            }
        }

        // Fallback: simple entry->exit CFG if we can't find the function
        let mut cfg = Cfg::new();
        cfg.add_edge(cfg.entry, cfg.exit);
        cfg
    }
}

/// Internal builder state for constructing the CFG
struct PythonCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    /// Stack of loop contexts for break/continue
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    break_target: NodeId,
    continue_target: NodeId,
}

impl PythonCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;

        PythonCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
        }
    }

    /// Build CFG from a block node
    fn build_from_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();

        for child in block.children(&mut cursor) {
            // Skip structural nodes, process only named children
            if child.is_named() {
                self.visit_node(&child, source);
            }
        }

        // Connect last node to exit
        if let Some(last_node) = self.current_node {
            if last_node != self.cfg.exit {
                let has_exit_edge = self.cfg.edges.iter()
                    .any(|e| e.from == last_node && e.to == self.cfg.exit);

                if !has_exit_edge {
                    self.cfg.add_edge(last_node, self.cfg.exit);
                }
            }
        }
    }

    /// Visit a tree-sitter node and build CFG
    fn visit_node(&mut self, node: &Node, source: &str) {
        match node.kind() {
            "if_statement" => self.visit_if(node, source),
            "while_statement" => self.visit_while(node, source),
            "for_statement" => self.visit_for(node, source),
            "try_statement" => self.visit_try(node, source),
            "with_statement" => self.visit_with(node, source),
            "match_statement" => self.visit_match(node, source),
            "return_statement" => self.visit_return(),
            "raise_statement" => self.visit_raise(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "assert_statement" => self.visit_assert(),
            "expression_statement" => self.visit_expression_statement(node, source),
            "assignment" => self.visit_simple_statement(),
            "augmented_assignment" => self.visit_simple_statement(),
            "block" => self.build_from_block(node, source),
            _ => {
                // Regular statement - add node and continue
                self.visit_simple_statement();
            }
        }
    }

    fn visit_simple_statement(&mut self) {
        if let Some(from_node) = self.current_node {
            let stmt_node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, stmt_node);
            self.current_node = Some(stmt_node);
        }
    }

    fn visit_if(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Condition node
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);

        // Then branch (consequence)
        let then_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, then_start);

        if let Some(consequence) = find_child_by_kind(*node, "block") {
            self.current_node = Some(then_start);
            self.build_from_block(&consequence, source);
        }
        let then_end = self.current_node.unwrap_or(then_start);

        // Handle elif_clause and else_clause
        let join_node = self.cfg.add_node(NodeKind::Join);
        let mut last_condition = condition_node;
        let mut branch_ends = vec![then_end];

        // Process elif clauses
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "elif_clause" {
                // Create condition node for elif
                let elif_condition = self.cfg.add_node(NodeKind::Condition);
                self.cfg.add_edge(last_condition, elif_condition);

                // elif body
                if let Some(elif_body) = find_child_by_kind(child, "block") {
                    let elif_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(elif_condition, elif_start);
                    self.current_node = Some(elif_start);
                    self.build_from_block(&elif_body, source);
                    branch_ends.push(self.current_node.unwrap_or(elif_start));
                }

                last_condition = elif_condition;
            } else if child.kind() == "else_clause" {
                // Else branch
                if let Some(else_body) = find_child_by_kind(child, "block") {
                    let else_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(last_condition, else_start);
                    self.current_node = Some(else_start);
                    self.build_from_block(&else_body, source);
                    branch_ends.push(self.current_node.unwrap_or(else_start));
                }
            }
        }

        // If no else clause, last condition can go directly to join
        self.cfg.add_edge(last_condition, join_node);

        // Connect all branch ends to join
        for end in branch_ends {
            if end != self.cfg.exit {
                self.cfg.add_edge(end, join_node);
            }
        }

        self.current_node = Some(join_node);
    }

    fn visit_while(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Loop header (condition)
        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, loop_header);

        // Loop body
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(loop_header, body_start);

        // Join node (after loop)
        let join_node = self.cfg.add_node(NodeKind::Join);

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_target: join_node,
            continue_target: loop_header,
        });

        // Visit loop body
        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(body_start);
            self.build_from_block(&body, source);

            // Back edge to loop header
            if let Some(body_end) = self.current_node {
                if body_end != self.cfg.exit {
                    self.cfg.add_edge(body_end, loop_header);
                }
            }
        }

        // Pop loop context
        self.loop_stack.pop();

        // Loop exit edge
        self.cfg.add_edge(loop_header, join_node);

        self.current_node = Some(join_node);
    }

    fn visit_for(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Loop header
        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, loop_header);

        // Loop body
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(loop_header, body_start);

        // Join node (after loop)
        let join_node = self.cfg.add_node(NodeKind::Join);

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_target: join_node,
            continue_target: loop_header,
        });

        // Visit loop body
        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(body_start);
            self.build_from_block(&body, source);

            // Back edge to loop header
            if let Some(body_end) = self.current_node {
                if body_end != self.cfg.exit {
                    self.cfg.add_edge(body_end, loop_header);
                }
            }
        }

        // Pop loop context
        self.loop_stack.pop();

        // Loop exit edge
        self.cfg.add_edge(loop_header, join_node);

        self.current_node = Some(join_node);
    }

    fn visit_try(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Try block
        let try_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from_node, try_start);

        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(try_start);
            self.build_from_block(&body, source);
        }
        let try_end = self.current_node.unwrap_or(try_start);

        // Join node (after all handlers)
        let join_node = self.cfg.add_node(NodeKind::Join);
        let mut branch_ends = vec![try_end];

        // Process except clauses
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "except_clause" {
                // Each except clause is a separate execution path
                let except_condition = self.cfg.add_node(NodeKind::Condition);
                self.cfg.add_edge(from_node, except_condition);

                if let Some(except_body) = find_child_by_kind(child, "block") {
                    let except_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(except_condition, except_start);
                    self.current_node = Some(except_start);
                    self.build_from_block(&except_body, source);
                    branch_ends.push(self.current_node.unwrap_or(except_start));
                }
            } else if child.kind() == "else_clause" {
                // Else clause runs if no exception was raised
                if let Some(else_body) = find_child_by_kind(child, "block") {
                    let else_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(try_end, else_start);
                    self.current_node = Some(else_start);
                    self.build_from_block(&else_body, source);
                    branch_ends.push(self.current_node.unwrap_or(else_start));
                }
            } else if child.kind() == "finally_clause" {
                // Finally clause always runs - creates a join point
                let finally_node = self.cfg.add_node(NodeKind::Statement);

                // Connect all branches to finally
                for &end in &branch_ends {
                    if end != self.cfg.exit {
                        self.cfg.add_edge(end, finally_node);
                    }
                }

                if let Some(finally_body) = find_child_by_kind(child, "block") {
                    self.current_node = Some(finally_node);
                    self.build_from_block(&finally_body, source);
                }

                // Finally becomes the new end
                branch_ends = vec![self.current_node.unwrap_or(finally_node)];
            }
        }

        // Connect all branch ends to join
        for end in branch_ends {
            if end != self.cfg.exit {
                self.cfg.add_edge(end, join_node);
            }
        }

        self.current_node = Some(join_node);
    }

    fn visit_with(&mut self, node: &Node, source: &str) {
        // With statement - context manager (doesn't add to CC, just ND)
        // Treat as simple statement followed by block
        self.visit_simple_statement();

        if let Some(body) = find_child_by_kind(*node, "block") {
            self.build_from_block(&body, source);
        }
    }

    fn visit_match(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Match expression evaluation
        let match_node = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from_node, match_node);

        // Join node (after all cases)
        let join_node = self.cfg.add_node(NodeKind::Join);

        // Process case clauses
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "case_clause" {
                // Each case is a separate execution path (adds to CC)
                let case_condition = self.cfg.add_node(NodeKind::Condition);
                self.cfg.add_edge(match_node, case_condition);

                if let Some(case_body) = find_child_by_kind(child, "block") {
                    let case_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(case_condition, case_start);
                    self.current_node = Some(case_start);
                    self.build_from_block(&case_body, source);

                    if let Some(case_end) = self.current_node {
                        if case_end != self.cfg.exit {
                            self.cfg.add_edge(case_end, join_node);
                        }
                    }
                }
            }
        }

        self.current_node = Some(join_node);
    }

    fn visit_return(&mut self) {
        if let Some(from_node) = self.current_node {
            self.cfg.add_edge(from_node, self.cfg.exit);
            self.current_node = Some(self.cfg.exit);
        }
    }

    fn visit_raise(&mut self) {
        // Raise statement - non-structured exit
        if let Some(from_node) = self.current_node {
            self.cfg.add_edge(from_node, self.cfg.exit);
            self.current_node = Some(self.cfg.exit);
        }
    }

    fn visit_break(&mut self) {
        if let Some(loop_ctx) = self.loop_stack.last() {
            if let Some(from_node) = self.current_node {
                self.cfg.add_edge(from_node, loop_ctx.break_target);
                self.current_node = Some(loop_ctx.break_target);
            }
        }
    }

    fn visit_continue(&mut self) {
        if let Some(loop_ctx) = self.loop_stack.last() {
            if let Some(from_node) = self.current_node {
                self.cfg.add_edge(from_node, loop_ctx.continue_target);
                self.current_node = Some(loop_ctx.continue_target);
            }
        }
    }

    fn visit_assert(&mut self) {
        // Assert is like a simple statement (doesn't add to CC)
        self.visit_simple_statement();
    }

    fn visit_expression_statement(&mut self, node: &Node, source: &str) {
        // Check for comprehensions with if-filters, ternary expressions, and boolean operators
        if has_control_flow_in_expression(node, source) {
            // Expression has decision points - add condition node
            if let Some(from_node) = self.current_node {
                let condition_node = self.cfg.add_node(NodeKind::Condition);
                self.cfg.add_edge(from_node, condition_node);
                self.current_node = Some(condition_node);
            }
        } else {
            // Simple expression
            self.visit_simple_statement();
        }
    }
}

/// Check if expression contains control flow (comprehensions with if, ternary, boolean operators)
fn has_control_flow_in_expression(node: &Node, _source: &str) -> bool {
    let mut cursor = node.walk();
    has_control_flow_recursive(node, &mut cursor)
}

fn has_control_flow_recursive<'a>(node: &Node<'a>, cursor: &mut tree_sitter::TreeCursor<'a>) -> bool {
    match node.kind() {
        // Comprehensions with if clause add to CC
        "list_comprehension" | "dictionary_comprehension" | "set_comprehension" | "generator_expression" => {
            // Check if it has an if_clause child
            for child in node.children(cursor) {
                if child.kind() == "if_clause" {
                    return true;
                }
            }
            false
        }
        // Ternary expression (conditional_expression) adds to CC
        "conditional_expression" => true,
        // Boolean operators (and, or) add to CC
        "boolean_operator" => true,
        _ => {
            // Recursively check children
            // Collect children first to avoid multiple mutable borrows
            let children: Vec<_> = node.children(cursor).collect();
            for child in children {
                let mut child_cursor = child.walk();
                if has_control_flow_recursive(&child, &mut child_cursor) {
                    return true;
                }
            }
            false
        }
    }
}

/// Find a function node by start position
fn find_function_by_start(node: Node, start: usize) -> Option<Node> {
    if (node.kind() == "function_definition" || node.kind() == "async_function_definition")
        && node.start_byte() == start
    {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_by_start(child, start) {
            return Some(found);
        }
    }

    None
}

/// Find a child node by kind
#[allow(clippy::manual_find)]
fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FunctionId, FunctionNode};
    use crate::language::{FunctionBody, SourceSpan};

    fn make_python_function(source: &str) -> FunctionNode {
        // Parse the source to find the actual function start position
        use tree_sitter::Parser;
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser.set_language(&language.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();

        // Find the function definition node
        let mut cursor = root.walk();
        let func_node = root.children(&mut cursor)
            .find(|n| n.kind() == "function_definition" || n.kind() == "async_function_definition")
            .expect("No function found in test source");

        let start_byte = func_node.start_byte();

        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test_func".to_string()),
            span: SourceSpan::new(start_byte, func_node.end_byte(), func_node.start_position().row as u32 + 1, func_node.start_position().column as u32),
            body: FunctionBody::Python {
                body_node: 0,
                source: source.to_string(),
            },
            suppression_reason: None,
        }
    }

    #[test]
    fn test_simple_function() {
        let source = r#"
def test_func():
    x = 1
    return x
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        // Should have entry, exit, and some nodes for statements
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
def test_func(x):
    if x > 0:
        return 1
    else:
        return 0
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        // Should have branching structure
        assert!(cfg.node_count() > 4);
        assert!(cfg.edge_count() > 4);
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
def test_func(n):
    while n > 0:
        n -= 1
    return n
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        // Should have loop structure with back edge
        assert!(cfg.node_count() >= 5);
    }

    #[test]
    fn test_for_loop() {
        let source = r#"
def test_func(items):
    for item in items:
        print(item)
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        // Should have loop structure
        assert!(cfg.node_count() >= 5);
    }
}
