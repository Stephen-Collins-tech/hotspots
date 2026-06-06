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
    break_target: Option<NodeId>,
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
            "goto_statement" => self.visit_goto(),
            "labeled_statement" => self.visit_labeled(node, source),
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

    /// Visit a statement body — compound blocks use build_from_block, single statements use visit_node.
    fn visit_body(&mut self, node: &Node, source: &str) {
        if node.kind() == "compound_statement" {
            self.build_from_block(node, source);
        } else {
            self.visit_node(node, source);
        }
    }

    /// Get the consequence (then-body) of an if_statement.
    /// Tries the "consequence" field name first, then falls back to position-based search.
    fn get_if_consequence<'a>(node: &Node<'a>) -> Option<Node<'a>> {
        if let Some(n) = node.child_by_field_name("consequence") {
            return Some(n);
        }
        // Fallback: first named child after the parenthesized condition
        let mut cursor = node.walk();
        let mut past_condition = false;
        for child in node.children(&mut cursor) {
            if child.kind() == "parenthesized_expression" {
                past_condition = true;
                continue;
            }
            if past_condition && child.is_named() && child.kind() != "else_clause" {
                return Some(child);
            }
        }
        None
    }

    fn visit_if(&mut self, node: &Node, source: &str) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);
        let join_node = self.cfg.add_node(NodeKind::Join);

        // Then branch
        if let Some(consequence) = Self::get_if_consequence(node) {
            let then_start = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(condition_node, then_start);
            self.current_node = Some(then_start);
            self.visit_body(&consequence, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, join_node);
                }
            }
        }

        // Else branch (via else_clause child)
        let mut found_else = false;
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for child in &children {
            if child.kind() == "else_clause" {
                found_else = true;
                // else_clause wraps the alternative — get its first named child
                let mut ec_cursor = child.walk();
                let alt_children: Vec<_> = child.children(&mut ec_cursor).collect();
                if let Some(alt) = alt_children.iter().find(|c| c.is_named()) {
                    let else_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, else_start);
                    self.current_node = Some(else_start);
                    self.visit_body(alt, source);
                    if let Some(end) = self.current_node {
                        if end != self.cfg.exit {
                            self.cfg.add_edge(end, join_node);
                        }
                    }
                }
            }
        }

        if !found_else {
            self.cfg.add_edge(condition_node, join_node);
        }

        self.current_node = Some(join_node);
    }

    fn visit_while(&mut self, node: &Node, source: &str) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, loop_header);

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(loop_header, body_start);

        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: loop_header,
        });

        let body = node
            .child_by_field_name("body")
            .or_else(|| find_child_by_kind(*node, "compound_statement"));
        if let Some(body) = body {
            self.current_node = Some(body_start);
            self.visit_body(&body, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, loop_header);
                }
            }
        } else {
            self.cfg.add_edge(body_start, loop_header);
        }

        let join_node = self.get_or_create_loop_break(self.loop_stack.len() - 1);
        self.loop_stack.pop();
        self.cfg.add_edge(loop_header, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_for(&mut self, node: &Node, source: &str) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, loop_header);

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(loop_header, body_start);

        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: loop_header,
        });

        let body = node
            .child_by_field_name("body")
            .or_else(|| find_child_by_kind(*node, "compound_statement"));
        if let Some(body) = body {
            self.current_node = Some(body_start);
            self.visit_body(&body, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, loop_header);
                }
            }
        } else {
            self.cfg.add_edge(body_start, loop_header);
        }

        let join_node = self.get_or_create_loop_break(self.loop_stack.len() - 1);
        self.loop_stack.pop();
        self.cfg.add_edge(loop_header, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_do_while(&mut self, node: &Node, source: &str) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from_node, body_start);

        let loop_header = self.cfg.add_node(NodeKind::LoopHeader);

        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: loop_header,
        });

        let body = node
            .child_by_field_name("body")
            .or_else(|| find_child_by_kind(*node, "compound_statement"));
        if let Some(body) = body {
            self.current_node = Some(body_start);
            self.visit_body(&body, source);
            if let Some(end) = self.current_node {
                if end != self.cfg.exit {
                    self.cfg.add_edge(end, loop_header);
                }
            }
        }

        let join_node = self.get_or_create_loop_break(self.loop_stack.len() - 1);
        self.loop_stack.pop();
        self.cfg.add_edge(loop_header, body_start);
        self.cfg.add_edge(loop_header, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_switch(&mut self, node: &Node, source: &str) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let switch_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, switch_node);

        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: switch_node,
        });

        let body = node
            .child_by_field_name("body")
            .or_else(|| find_child_by_kind(*node, "compound_statement"));
        if let Some(body) = body {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "case_statement" || child.kind() == "default_statement" {
                    let case_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(switch_node, case_start);
                    self.current_node = Some(case_start);
                    let mut case_cursor = child.walk();
                    for case_child in child.children(&mut case_cursor) {
                        if case_child.is_named() && case_child.kind() != ":" {
                            self.visit_node(&case_child, source);
                        }
                    }
                    let idx = self.loop_stack.len() - 1;
                    let join = self.get_or_create_loop_break(idx);
                    if let Some(end) = self.current_node {
                        if end != self.cfg.exit {
                            self.cfg.add_edge(end, join);
                        }
                    }
                }
            }
        }

        let join_node = self.get_or_create_loop_break(self.loop_stack.len() - 1);
        self.loop_stack.pop();
        self.cfg.add_edge(switch_node, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_return(&mut self) {
        if let Some(from_node) = self.current_node {
            self.cfg.add_edge(from_node, self.cfg.exit);
            self.current_node = None;
        }
    }

    fn visit_goto(&mut self) {
        if let Some(from_node) = self.current_node {
            // goto is a branch: +1 CC, then control leaves this path
            let goto_node = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(from_node, goto_node);
            self.cfg.add_edge(goto_node, self.cfg.exit);
            self.current_node = None;
        }
    }

    fn visit_labeled(&mut self, node: &Node, source: &str) {
        // A label is a jump target reachable via goto. Always create a node for it.
        let label_node = self.cfg.add_node(NodeKind::Statement);
        if let Some(from) = self.current_node {
            self.cfg.add_edge(from, label_node);
        } else {
            // Dead code path (after goto/return) — connect from entry so the CFG
            // remains valid without inflating CC (adds one node and one edge: net 0).
            self.cfg.add_edge(self.cfg.entry, label_node);
        }
        self.current_node = Some(label_node);
        // Visit the labeled body (skip the statement_identifier child)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() && child.kind() != "statement_identifier" {
                self.visit_node(&child, source);
                break;
            }
        }
    }

    fn visit_break(&mut self) {
        if let Some(from_node) = self.current_node {
            let idx = self.loop_stack.len().wrapping_sub(1);
            let target = self.get_or_create_loop_break(idx);
            self.cfg.add_edge(from_node, target);
            self.current_node = None;
        }
    }

    fn visit_continue(&mut self) {
        if let Some(loop_ctx) = self.loop_stack.last() {
            if let Some(from_node) = self.current_node {
                let target = loop_ctx.continue_target;
                self.cfg.add_edge(from_node, target);
                self.current_node = None;
            }
        }
    }

    /// Get or lazily create the break/join target for the loop at `idx`.
    fn get_or_create_loop_break(&mut self, idx: usize) -> NodeId {
        if let Some(ctx) = self.loop_stack.get(idx) {
            if let Some(id) = ctx.break_target {
                return id;
            }
        }
        let id = self.cfg.add_node(NodeKind::Join);
        if let Some(ctx) = self.loop_stack.get_mut(idx) {
            ctx.break_target = Some(id);
        }
        id
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

    fn cc(source: &str) -> usize {
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        // CC = E - N + 2
        (cfg.edge_count() as isize - cfg.node_count() as isize + 2).max(1) as usize
    }

    #[test]
    fn test_simple_function() {
        let source = "int test_func(int x) { return x + 1; }";
        assert_eq!(cc(source), 1);
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
        assert_eq!(cc(source), 2);
    }

    #[test]
    fn test_if_no_else() {
        let source = r#"
int test_func(int x) {
    if (x > 0) {
        x = 0;
    }
    return x;
}
"#;
        assert_eq!(cc(source), 2);
    }

    #[test]
    fn test_braceless_if() {
        let source = r#"
int test_func(int x) {
    if (x > 0)
        return 1;
    return 0;
}
"#;
        assert_eq!(cc(source), 2);
    }

    #[test]
    fn test_braceless_if_else() {
        let source = r#"
int test_func(int x) {
    if (x > 0)
        return 1;
    else
        return 0;
}
"#;
        assert_eq!(cc(source), 2);
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
        assert_eq!(cc(source), 2);
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
        assert_eq!(cc(source), 2);
    }

    #[test]
    fn test_do_while_loop() {
        let source = r#"
void test_func(int n) {
    do {
        n--;
    } while (n > 0);
}
"#;
        assert_eq!(cc(source), 2);
    }

    #[test]
    fn test_switch() {
        let source = r#"
void test_func(int x) {
    switch (x) {
        case 1: break;
        case 2: break;
        default: break;
    }
}
"#;
        // switch condition node + 3 case edges + default fallthrough = CC 4
        let c = cc(source);
        assert!(c >= 3, "switch with 3 cases should have CC >= 3, got {}", c);
    }

    #[test]
    fn test_goto_adds_branch() {
        // goto should add +1 CC (branch node)
        let source_no_goto = r#"
int test_func(int x) {
    return x;
}
"#;
        let source_with_goto = r#"
int test_func(int x) {
    if (x < 0) goto done;
    x = x + 1;
    done:
    return x;
}
"#;
        let cc_base = cc(source_no_goto);
        let cc_goto = cc(source_with_goto);
        // if + goto = at least 2 more than base
        assert!(
            cc_goto > cc_base,
            "goto should increase CC: base={}, goto={}",
            cc_base,
            cc_goto
        );
    }

    #[test]
    fn test_labeled_statement_after_goto() {
        // Label after goto should not panic (dead code before label)
        let source = r#"
void test_func(void) {
    goto end;
    end:
    return;
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_break_in_loop() {
        let source = r#"
void test_func(int n) {
    while (1) {
        if (n <= 0) break;
        n--;
    }
}
"#;
        let c = cc(source);
        assert!(c >= 3, "while+if+break should have CC >= 3, got {}", c);
    }

    #[test]
    fn test_continue_in_loop() {
        let source = r#"
void test_func(int n) {
    int i;
    for (i = 0; i < n; i++) {
        if (i % 2 == 0) continue;
        n--;
    }
}
"#;
        let c = cc(source);
        assert!(c >= 3, "for+if+continue should have CC >= 3, got {}", c);
    }

    #[test]
    fn test_dead_code_after_return_no_panic() {
        // Dead code after return should not panic
        let source = r#"
int test_func(int x) {
    return x;
    if (x > 0) {
        return 1;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    // --- aggressive edge-case tests ---

    #[test]
    fn test_break_outside_loop_no_panic() {
        // Malformed: break with no enclosing loop — must not panic or index-overflow
        let source = r#"
void test_func(void) {
    break;
    return;
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_infinite_for_always_breaks() {
        // Infinite for whose body always breaks — join node created lazily
        let source = r#"
void test_func(int n) {
    for (;;) {
        if (n <= 0) break;
        n--;
        break;
    }
}
"#;
        let c = cc(source);
        assert!(
            c >= 2,
            "infinite for with breaks should have CC >= 2, got {}",
            c
        );
    }

    #[test]
    fn test_chained_else_if() {
        let source = r#"
int test_func(int x) {
    if (x < 0) return -1;
    else if (x == 0) return 0;
    else if (x < 10) return 1;
    else return 2;
}
"#;
        // 3 conditions → CC 4
        assert_eq!(cc(source), 4);
    }

    #[test]
    fn test_nested_loops_break_continue() {
        let source = r#"
void test_func(int n) {
    int i, j;
    for (i = 0; i < n; i++) {
        for (j = 0; j < n; j++) {
            if (j == 0) continue;
            if (j > 5) break;
        }
    }
}
"#;
        let c = cc(source);
        assert!(
            c >= 5,
            "nested loops with break+continue should have CC >= 5, got {}",
            c
        );
    }

    #[test]
    fn test_switch_fallthrough() {
        // Cases without break fall through — CFG should not panic
        let source = r#"
void test_func(int x) {
    switch (x) {
        case 1:
        case 2:
            x = 0;
            break;
        case 3:
            x = 1;
        default:
            x = -1;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_switch_body_always_returns() {
        // Every case returns — join node created lazily, CFG stays valid
        let source = r#"
int test_func(int x) {
    switch (x) {
        case 1: return 1;
        case 2: return 2;
        default: return 0;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_empty_switch() {
        let source = r#"
void test_func(int x) {
    switch (x) {}
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_multiple_gotos_same_label() {
        let source = r#"
int test_func(int x, int y) {
    if (x < 0) goto fail;
    if (y < 0) goto fail;
    if (x + y > 100) goto fail;
    return 0;
    fail:
    return -1;
}
"#;
        let c = cc(source);
        // 3 ifs + 3 gotos = at least 4
        assert!(c >= 4, "multiple gotos should contribute to CC, got {}", c);
    }

    #[test]
    fn test_label_dead_code_then_more_code() {
        // After goto skips to label, execution continues past label
        let source = r#"
int test_func(int x) {
    goto mid;
    x = x * 2;
    mid:
    x = x + 1;
    return x;
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_consecutive_returns_no_panic() {
        // Dead code: two returns in a row
        let source = r#"
int test_func(int x) {
    return x;
    return x + 1;
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_do_while_body_always_returns() {
        // do-while where body always returns — join node never needed
        let source = r#"
int test_func(int x) {
    do {
        return x;
    } while (x > 0);
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_while_body_always_returns() {
        let source = r#"
int test_func(int x) {
    while (x > 0) {
        return x;
    }
    return 0;
}
"#;
        let c = cc(source);
        assert!(c >= 2, "while should have CC >= 2, got {}", c);
    }

    #[test]
    fn test_deeply_nested_if() {
        let source = r#"
int test_func(int a, int b, int c, int d) {
    if (a > 0) {
        if (b > 0) {
            if (c > 0) {
                if (d > 0) return 1;
                return 2;
            }
            return 3;
        }
        return 4;
    }
    return 5;
}
"#;
        assert_eq!(cc(source), 5);
    }

    #[test]
    fn test_goto_after_goto_no_panic() {
        // Two consecutive gotos — second is dead code
        let source = r#"
void test_func(void) {
    goto a;
    goto b;
    a:
    b:
    return;
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_braceless_nested_if_else() {
        let source = r#"
int test_func(int x, int y) {
    if (x > 0)
        if (y > 0)
            return 1;
        else
            return 2;
    return 0;
}
"#;
        assert_eq!(cc(source), 3);
    }

    #[test]
    fn test_for_no_condition_with_break() {
        // for(;;) with explicit break — lazy join must fire
        let source = r#"
int test_func(int x) {
    for (;;) {
        break;
    }
    return x;
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_continue_in_do_while() {
        let source = r#"
void test_func(int n) {
    do {
        if (n % 2 == 0) continue;
        n--;
    } while (n > 0);
}
"#;
        let c = cc(source);
        assert!(
            c >= 3,
            "do-while+if+continue should have CC >= 3, got {}",
            c
        );
    }
}
