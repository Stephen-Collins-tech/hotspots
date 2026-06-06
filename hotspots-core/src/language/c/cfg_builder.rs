//! C CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use crate::language::tree_sitter_utils::{
    find_child_by_kind, find_function_by_start, with_cached_c_tree,
};
use tree_sitter::Node;

/// C CFG builder
pub struct CCfgBuilder;

impl CfgBuilder for CCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_c();

        let result = with_cached_c_tree(source, |root| {
            let func_node =
                find_function_by_start(root, function.span.start, &["function_definition"])?;
            let body_node = find_child_by_kind(func_node, "compound_statement")?;
            let mut builder = CCfgBuilderState::new();
            builder.build_from_block(&body_node, source);
            // Connect final node to exit
            if let Some(last) = builder.current_node {
                if last != builder.cfg.exit {
                    builder.cfg.add_edge(last, builder.cfg.exit);
                }
            }
            Some(builder.cfg)
        });

        result.unwrap_or_else(|| {
            let mut cfg = Cfg::new();
            cfg.add_edge(cfg.entry, cfg.exit);
            cfg
        })
    }
}

struct LoopContext {
    break_target: NodeId,
    continue_target: NodeId,
}

struct CCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    loop_stack: Vec<LoopContext>,
}

impl CCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;
        CCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
        }
    }

    fn build_from_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();
        for child in block.children(&mut cursor) {
            if child.is_named() {
                self.visit_node(&child, source);
            }
        }
        // Callers are responsible for connecting self.current_node to whatever comes next.
        // The top-level build() connects to exit after this returns.
    }

    fn visit_node(&mut self, node: &Node, source: &str) {
        match node.kind() {
            "if_statement" => self.visit_if(node, source),
            "while_statement" => self.visit_while(node, source),
            "for_statement" => self.visit_for(node, source),
            "do_statement" => self.visit_do_while(node, source),
            "switch_statement" => self.visit_switch(node, source),
            "return_statement" => self.visit_return(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "goto_statement" => self.visit_return(), // treat goto as exit for CC purposes
            "compound_statement" => self.build_from_block(node, source),
            _ => self.visit_simple_statement(),
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
        let from_node = self.current_node.expect("current node should exist");

        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);

        let join_node = self.cfg.add_node(NodeKind::Join);

        // Then branch — the first compound_statement child
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        let mut found_consequence = false;
        let mut found_else = false;

        for child in &children {
            if child.kind() == "compound_statement" && !found_consequence {
                found_consequence = true;
                let then_start = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(condition_node, then_start);
                self.current_node = Some(then_start);
                self.build_from_block(child, source);
                if let Some(end) = self.current_node {
                    if end != self.cfg.exit {
                        self.cfg.add_edge(end, join_node);
                    }
                }
            } else if child.kind() == "else_clause" {
                found_else = true;
                // else_clause contains a compound_statement or another if_statement
                let else_body = find_child_by_kind(*child, "compound_statement")
                    .or_else(|| find_child_by_kind(*child, "if_statement"));
                if let Some(body) = else_body {
                    let else_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, else_start);
                    self.current_node = Some(else_start);
                    if body.kind() == "if_statement" {
                        self.visit_if(&body, source);
                    } else {
                        self.build_from_block(&body, source);
                    }
                    if let Some(end) = self.current_node {
                        if end != self.cfg.exit {
                            self.cfg.add_edge(end, join_node);
                        }
                    }
                }
            }
        }

        // No else → condition can fall through to join
        if !found_else {
            self.cfg.add_edge(condition_node, join_node);
        }

        self.current_node = Some(join_node);
    }

    fn visit_while(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("current node should exist");

        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, loop_header);

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(loop_header, body_start);

        let join_node = self.cfg.add_node(NodeKind::Join);
        self.loop_stack.push(LoopContext {
            break_target: join_node,
            continue_target: loop_header,
        });

        if let Some(body) = find_child_by_kind(*node, "compound_statement") {
            self.current_node = Some(body_start);
            self.build_from_block(&body, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, loop_header);
                }
            }
        }

        self.loop_stack.pop();
        self.cfg.add_edge(loop_header, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_for(&mut self, node: &Node, source: &str) {
        // Model for-loop the same as while: a single loop_header condition node.
        // The init and increment expressions don't affect CC, only the condition does.
        let from_node = self.current_node.expect("current node should exist");

        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, loop_header);

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(loop_header, body_start);

        let join_node = self.cfg.add_node(NodeKind::Join);
        self.loop_stack.push(LoopContext {
            break_target: join_node,
            continue_target: loop_header,
        });

        if let Some(body) = find_child_by_kind(*node, "compound_statement") {
            self.current_node = Some(body_start);
            self.build_from_block(&body, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, loop_header);
                }
            }
        } else {
            self.cfg.add_edge(body_start, loop_header);
        }

        self.loop_stack.pop();
        self.cfg.add_edge(loop_header, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_do_while(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("current node should exist");

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from_node, body_start);

        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        let join_node = self.cfg.add_node(NodeKind::Join);

        self.loop_stack.push(LoopContext {
            break_target: join_node,
            continue_target: loop_header,
        });

        if let Some(body) = find_child_by_kind(*node, "compound_statement") {
            self.current_node = Some(body_start);
            self.build_from_block(&body, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, loop_header);
                }
            }
        }

        self.loop_stack.pop();
        // condition at loop_header: back edge or exit
        self.cfg.add_edge(loop_header, body_start);
        self.cfg.add_edge(loop_header, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_switch(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("current node should exist");

        let switch_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, switch_node);

        let join_node = self.cfg.add_node(NodeKind::Join);
        self.loop_stack.push(LoopContext {
            break_target: join_node,
            continue_target: join_node, // switch doesn't have a natural continue target
        });

        if let Some(body) = find_child_by_kind(*node, "compound_statement") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "case_statement" || child.kind() == "default_statement" {
                    let case_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(switch_node, case_start);
                    self.current_node = Some(case_start);
                    // Visit children of the case
                    let mut case_cursor = child.walk();
                    for case_child in child.children(&mut case_cursor) {
                        if case_child.is_named() && case_child.kind() != ":" {
                            self.visit_node(&case_child, source);
                        }
                    }
                    if let Some(end) = self.current_node {
                        if end != self.cfg.exit {
                            self.cfg.add_edge(end, join_node);
                        }
                    }
                }
            }
        }

        self.loop_stack.pop();
        self.cfg.add_edge(switch_node, join_node); // default fallthrough
        self.current_node = Some(join_node);
    }

    fn visit_return(&mut self) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FunctionId, FunctionNode};
    use crate::language::{FunctionBody, SourceSpan};

    fn make_c_function(source: &str) -> FunctionNode {
        use tree_sitter::Parser;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();

        let mut cursor = root.walk();
        let func_node = root
            .children(&mut cursor)
            .find(|n| n.kind() == "function_definition")
            .expect("No function found in test source");

        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test_func".to_string()),
            span: SourceSpan::new(
                func_node.start_byte(),
                func_node.end_byte(),
                func_node.start_position().row as u32 + 1,
                func_node.end_position().row as u32 + 1,
                func_node.start_position().column as u32,
            ),
            body: FunctionBody::C {
                body_node: 0,
                source: source.to_string(),
            },
            suppression_reason: None,
        }
    }

    #[test]
    fn test_simple_function() {
        let source = "int test_func(int x) { return x + 1; }";
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
int test_func(int x) {
    if (x > 0) {
        return 1;
    } else {
        return 0;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() > 4);
        assert!(cfg.edge_count() > 4);
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
void test_func(int n) {
    while (n > 0) {
        n--;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 4);
    }

    #[test]
    fn test_for_loop() {
        let source = r#"
void test_func(int n) {
    int i;
    for (i = 0; i < n; i++) {
        n--;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 5);
    }
}
