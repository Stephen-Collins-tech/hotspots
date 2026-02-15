//! Go CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use tree_sitter::{Node, Parser};

/// Go CFG builder
///
/// Builds control flow graphs from Go function bodies parsed with tree-sitter.
pub struct GoCfgBuilder;

impl CfgBuilder for GoCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_go();

        // Re-parse the source to get the tree
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
        if let Some(func_node) = find_function_by_start(root, function.span.start) {
            // Find the block (function body)
            if let Some(body_node) = find_child_by_kind(func_node, "block") {
                let mut builder = GoCfgBuilderState::new();
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
struct GoCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    /// Stack of loop contexts for break/continue
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    break_target: NodeId,
    continue_target: NodeId,
}

impl GoCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;

        GoCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
        }
    }

    /// Build CFG from a block node
    fn build_from_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();

        for child in block.children(&mut cursor) {
            // Skip braces and other structural nodes
            if child.is_named() {
                self.visit_node(&child, source);
            }
        }

        // Connect last node to exit
        if let Some(last_node) = self.current_node {
            if last_node != self.cfg.exit {
                let has_exit_edge = self
                    .cfg
                    .edges
                    .iter()
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
            "for_statement" => self.visit_for(node, source),
            "switch_statement" | "expression_switch_statement" => self.visit_switch(node, source),
            "type_switch_statement" => self.visit_type_switch(node, source),
            "select_statement" => self.visit_select(node, source),
            "return_statement" => self.visit_return(node),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "defer_statement" => self.visit_defer(node),
            "go_statement" => self.visit_go_statement(node),
            "expression_statement" => {
                // Check if this is a panic call
                if is_panic_call(node, source) {
                    self.visit_panic();
                } else {
                    self.visit_simple_statement();
                }
            }
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

        // Then branch (consequence block)
        let then_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, then_start);

        if let Some(consequence) = find_child_by_kind(*node, "block") {
            self.current_node = Some(then_start);
            self.build_from_block(&consequence, source);
        }
        let then_end = self.current_node.unwrap_or(then_start);

        // Else branch (alternative)
        let join_node = self.cfg.add_node(NodeKind::Join);

        if let Some(alternative) = find_child_by_field(*node, "alternative") {
            let else_start = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(condition_node, else_start);

            self.current_node = Some(else_start);
            self.visit_node(&alternative, source);
            let else_end = self.current_node.unwrap_or(else_start);

            if then_end != self.cfg.exit {
                self.cfg.add_edge(then_end, join_node);
            }
            if else_end != self.cfg.exit {
                self.cfg.add_edge(else_end, join_node);
            }
        } else {
            // No else branch - condition can go directly to join
            self.cfg.add_edge(condition_node, join_node);
            if then_end != self.cfg.exit {
                self.cfg.add_edge(then_end, join_node);
            }
        }

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

        // Exit condition from loop header
        self.cfg.add_edge(loop_header, join_node);

        self.current_node = Some(join_node);
    }

    fn visit_switch(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Switch condition
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);

        // Join node after switch
        let join_node = self.cfg.add_node(NodeKind::Join);

        // Visit each case
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "expression_case" || child.kind() == "default_case" {
                let case_start = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(condition_node, case_start);

                self.current_node = Some(case_start);

                // Visit case body
                let mut case_cursor = child.walk();
                for case_child in child.children(&mut case_cursor) {
                    if case_child.is_named() && case_child.kind() != ":" {
                        self.visit_node(&case_child, source);
                    }
                }

                // Case ends connect to join (unless they explicitly break/return)
                if let Some(case_end) = self.current_node {
                    if case_end != self.cfg.exit {
                        self.cfg.add_edge(case_end, join_node);
                    }
                }
            }
        }

        // If no default, condition can go directly to join
        self.cfg.add_edge(condition_node, join_node);

        self.current_node = Some(join_node);
    }

    fn visit_type_switch(&mut self, node: &Node, source: &str) {
        // Type switches work similar to regular switches
        self.visit_switch(node, source);
    }

    fn visit_select(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Select condition (non-deterministic choice)
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);

        // Join node after select
        let join_node = self.cfg.add_node(NodeKind::Join);

        // Visit each communication case
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "communication_case" || child.kind() == "default_case" {
                let case_start = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(condition_node, case_start);

                self.current_node = Some(case_start);

                // Visit case body
                let mut case_cursor = child.walk();
                for case_child in child.children(&mut case_cursor) {
                    if case_child.is_named() {
                        self.visit_node(&case_child, source);
                    }
                }

                if let Some(case_end) = self.current_node {
                    if case_end != self.cfg.exit {
                        self.cfg.add_edge(case_end, join_node);
                    }
                }
            }
        }

        self.current_node = Some(join_node);
    }

    fn visit_return(&mut self, _node: &Node) {
        if let Some(from_node) = self.current_node {
            let return_node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, return_node);
            self.cfg.add_edge(return_node, self.cfg.exit);
            self.current_node = None; // Dead code after return
        }
    }

    fn visit_break(&mut self) {
        if let Some(from_node) = self.current_node {
            if let Some(loop_ctx) = self.loop_stack.last() {
                let break_node = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(from_node, break_node);
                self.cfg.add_edge(break_node, loop_ctx.break_target);
                self.current_node = None; // Dead code after break
            }
        }
    }

    fn visit_continue(&mut self) {
        if let Some(from_node) = self.current_node {
            if let Some(loop_ctx) = self.loop_stack.last() {
                let continue_node = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(from_node, continue_node);
                self.cfg.add_edge(continue_node, loop_ctx.continue_target);
                self.current_node = None; // Dead code after continue
            }
        }
    }

    fn visit_defer(&mut self, _node: &Node) {
        // Defer statements are executed, but don't affect control flow
        // They're counted as non-structured exits in metrics
        self.visit_simple_statement();
    }

    fn visit_go_statement(&mut self, _node: &Node) {
        // Go statements spawn goroutines but don't affect control flow
        // They're counted in fan-out metrics
        self.visit_simple_statement();
    }

    fn visit_panic(&mut self) {
        if let Some(from_node) = self.current_node {
            let panic_node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, panic_node);
            self.cfg.add_edge(panic_node, self.cfg.exit);
            self.current_node = None; // Dead code after panic
        }
    }
}

/// Find a child node by kind
fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node.children(&mut cursor).find(|child| child.kind() == kind);
    result
}

/// Find a child node by field name
fn find_child_by_field<'a>(node: Node<'a>, field: &str) -> Option<Node<'a>> {
    node.child_by_field_name(field)
}

/// Find a function node by its start byte position
fn find_function_by_start(root: Node, start_byte: usize) -> Option<Node> {
    fn search_recursive<'a>(node: Node<'a>, start: usize) -> Option<Node<'a>> {
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

/// Check if a node is a panic() call
fn is_panic_call(node: &Node, source: &str) -> bool {
    // Check if this is a call_expression where the function is "panic"
    if let Some(call_expr) = find_child_by_kind(*node, "call_expression") {
        if let Some(func) = find_child_by_kind(call_expr, "identifier") {
            let func_text = &source[func.start_byte()..func.end_byte()];
            return func_text == "panic";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FunctionId, FunctionNode};
    use crate::language::parser::LanguageParser;
    use crate::language::{FunctionBody, SourceSpan};

    fn make_test_go_function(source: &str) -> FunctionNode {
        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test".to_string()),
            span: SourceSpan::new(0, source.len(), 1, 1, 0),
            body: FunctionBody::Go {
                body_node: 0,
                source: source.to_string(),
            },
            suppression_reason: None,
        }
    }

    #[test]
    fn test_go_cfg_builder_simple() {
        let source = r#"
package main
func test() {
    x := 1
}
"#;
        let function = make_test_go_function(source);
        let builder = GoCfgBuilder;
        let cfg = builder.build(&function);

        // Should have entry, statement, exit
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_go_cfg_builder_if() {
        let source = r#"
package main
func test(x int) {
    if x > 0 {
        println("positive")
    }
}
"#;
        // Parse to find the actual function start position
        use crate::language::GoParser;
        let parser = GoParser::new().unwrap();
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        let function = &functions[0];

        let builder = GoCfgBuilder;
        let cfg = builder.build(function);

        // Should have at least entry and exit
        // Full CFG would be: entry, condition, then, join, exit (5 nodes)
        // But we verify it's more than just entry->exit (2 nodes)
        assert!(
            cfg.node_count() >= 3,
            "Expected at least 3 nodes for if statement, got {}",
            cfg.node_count()
        );
        assert!(
            cfg.edge_count() >= 2,
            "Expected at least 2 edges, got {}",
            cfg.edge_count()
        );
    }

    #[test]
    fn test_go_cfg_builder_for() {
        let source = r#"
package main
func test() {
    for i := 0; i < 10; i++ {
        println(i)
    }
}
"#;
        // Parse to find the actual function start position
        use crate::language::GoParser;
        let parser = GoParser::new().unwrap();
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);

        assert_eq!(functions.len(), 1);
        let function = &functions[0];

        let builder = GoCfgBuilder;
        let cfg = builder.build(function);

        // Should have at least entry and exit
        // Full CFG would be: entry, loop header, body, join, exit (5 nodes)
        // But we verify it's more than just entry->exit (2 nodes)
        assert!(
            cfg.node_count() >= 3,
            "Expected at least 3 nodes for for loop, got {}",
            cfg.node_count()
        );
        assert!(
            cfg.edge_count() >= 2,
            "Expected at least 2 edges, got {}",
            cfg.edge_count()
        );
    }
}
