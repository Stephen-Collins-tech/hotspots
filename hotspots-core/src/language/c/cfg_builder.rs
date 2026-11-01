//! C CFG builder implementation
//!
//! Walks the tree-sitter-c AST iteratively via an explicit heap-allocated
//! work stack (`Task`, driven by the `run` loop, with results threaded
//! through `self.current_node`) instead of native function-call recursion,
//! so nesting depth is bounded by heap memory rather than the thread stack
//! (https://github.com/Stephen-Collins-tech/hotspots/issues/160).

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
            builder.build_from_block(body_node);
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

/// Pending unit of work on the explicit stack.
///
/// The start variants (`If`, `Loop`, `DoWhile`, `Switch`) do what the old
/// recursive `visit_*` methods did on the way down; the `*Finish`/`*After*`
/// variants do what they did with the nested result on the way back up. The
/// "result" of nested work is read from `self.current_node`, exactly as the
/// recursive methods used it.
enum Task<'a> {
    /// Visit each named child of a block, in order.
    Block {
        block: Node<'a>,
    },
    VisitNode {
        node: Node<'a>,
    },
    If {
        node: Node<'a>,
    },
    IfAfterThen {
        else_clauses: Vec<Node<'a>>,
        condition_node: NodeId,
        join_node: Option<NodeId>,
        had_then: bool,
    },
    /// Walk the `else_clause` children of an `if`, one at a time.
    IfElse {
        else_clauses: Vec<Node<'a>>,
        idx: usize,
        condition_node: NodeId,
        join_node: Option<NodeId>,
        found_else: bool,
        /// Whether a branch body just ran and its end needs joining
        pending: bool,
    },
    /// Shared by `while` and `for`.
    Loop {
        node: Node<'a>,
    },
    LoopFinish {
        loop_header: NodeId,
        had_body: bool,
    },
    DoWhile {
        node: Node<'a>,
    },
    DoWhileFinish {
        body_start: NodeId,
        loop_header: NodeId,
        had_body: bool,
    },
    Switch {
        node: Node<'a>,
    },
    CaseStart {
        switch_node: NodeId,
    },
    CaseFinish,
    SwitchFinish {
        switch_node: NodeId,
    },
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

    fn build_from_block(&mut self, block: Node<'_>) {
        let mut stack = vec![Task::Block { block }];
        self.run(&mut stack);
    }

    /// Lazily create the join node for an `if`, only once a branch needs it.
    fn get_join(&mut self, join_node: &mut Option<NodeId>) -> NodeId {
        *join_node.get_or_insert_with(|| self.cfg.add_node(NodeKind::Join))
    }

    /// Edge from the current node to the (lazily created) join, if the path is live.
    fn join_current(&mut self, join_node: &mut Option<NodeId>) {
        if let Some(end) = self.current_node {
            if end != self.cfg.exit {
                let j = self.get_join(join_node);
                self.cfg.add_edge(end, j);
            }
        }
    }

    /// Drive the explicit work-stack loop. Tasks are pushed in reverse so
    /// that they pop in source order.
    fn run<'a>(&mut self, stack: &mut Vec<Task<'a>>) {
        while let Some(task) = stack.pop() {
            match task {
                Task::Block { block } => {
                    for child in named_children(&block).into_iter().rev() {
                        stack.push(Task::VisitNode { node: child });
                    }
                }
                Task::VisitNode { node } => self.dispatch_node(node, stack),
                Task::If { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };

                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    let else_clauses = children_of_kind(&node, "else_clause");
                    let consequence = Self::get_if_consequence(&node);
                    stack.push(Task::IfAfterThen {
                        else_clauses,
                        condition_node,
                        join_node: None,
                        had_then: consequence.is_some(),
                    });
                    // Then branch
                    if let Some(consequence) = consequence {
                        let then_start = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(condition_node, then_start);
                        self.current_node = Some(then_start);
                        stack.push(Task::VisitNode { node: consequence });
                    }
                }
                Task::IfAfterThen {
                    else_clauses,
                    condition_node,
                    mut join_node,
                    had_then,
                } => {
                    if had_then {
                        self.join_current(&mut join_node);
                    }
                    let found_else = !else_clauses.is_empty();
                    stack.push(Task::IfElse {
                        else_clauses,
                        idx: 0,
                        condition_node,
                        join_node,
                        found_else,
                        pending: false,
                    });
                }
                Task::IfElse {
                    else_clauses,
                    idx,
                    condition_node,
                    mut join_node,
                    found_else,
                    pending,
                } => {
                    if pending {
                        self.join_current(&mut join_node);
                    }

                    let Some(&clause) = else_clauses.get(idx) else {
                        // No else: condition always has a fallthrough path to join.
                        if !found_else {
                            let j = self.get_join(&mut join_node);
                            self.cfg.add_edge(condition_node, j);
                        }
                        self.current_node = join_node;
                        continue;
                    };

                    let alt = named_children(&clause).into_iter().next();
                    if alt.is_some() {
                        let else_start = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(condition_node, else_start);
                        self.current_node = Some(else_start);
                    }
                    stack.push(Task::IfElse {
                        else_clauses,
                        idx: idx + 1,
                        condition_node,
                        join_node,
                        found_else,
                        pending: alt.is_some(),
                    });
                    if let Some(alt) = alt {
                        stack.push(Task::VisitNode { node: alt });
                    }
                }
                Task::Loop { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };

                    let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, loop_header);

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(loop_header, body_start);

                    self.loop_stack.push(LoopContext {
                        break_target: None,
                        continue_target: loop_header,
                    });

                    let body = Self::loop_body(&node);
                    stack.push(Task::LoopFinish {
                        loop_header,
                        had_body: body.is_some(),
                    });
                    if let Some(body) = body {
                        self.current_node = Some(body_start);
                        stack.push(Task::VisitNode { node: body });
                    } else {
                        self.cfg.add_edge(body_start, loop_header);
                    }
                }
                Task::LoopFinish {
                    loop_header,
                    had_body,
                } => {
                    if had_body {
                        if let Some(end) = self.current_node {
                            if end != self.cfg.exit {
                                self.cfg.add_edge(end, loop_header);
                            }
                        }
                    }

                    let join_node = self.get_or_create_loop_break(self.loop_stack.len() - 1);
                    self.loop_stack.pop();
                    self.cfg.add_edge(loop_header, join_node);
                    self.current_node = Some(join_node);
                }
                Task::DoWhile { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(from_node, body_start);

                    let loop_header = self.cfg.add_node(NodeKind::LoopHeader);

                    self.loop_stack.push(LoopContext {
                        break_target: None,
                        continue_target: loop_header,
                    });

                    let body = Self::loop_body(&node);
                    stack.push(Task::DoWhileFinish {
                        body_start,
                        loop_header,
                        had_body: body.is_some(),
                    });
                    if let Some(body) = body {
                        self.current_node = Some(body_start);
                        stack.push(Task::VisitNode { node: body });
                    }
                }
                Task::DoWhileFinish {
                    body_start,
                    loop_header,
                    had_body,
                } => {
                    if had_body {
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
                Task::Switch { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };

                    let switch_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, switch_node);

                    self.loop_stack.push(LoopContext {
                        break_target: None,
                        continue_target: switch_node,
                    });

                    stack.push(Task::SwitchFinish { switch_node });
                    if let Some(body) = Self::loop_body(&node) {
                        let cases: Vec<Node<'a>> = {
                            let mut cursor = body.walk();
                            body.children(&mut cursor)
                                .filter(|c| {
                                    c.kind() == "case_statement" || c.kind() == "default_statement"
                                })
                                .collect()
                        };
                        for case in cases.into_iter().rev() {
                            stack.push(Task::CaseFinish);
                            let mut cursor = case.walk();
                            let stmts: Vec<Node<'a>> = case
                                .children(&mut cursor)
                                .filter(|c| c.is_named() && c.kind() != ":")
                                .collect();
                            for stmt in stmts.into_iter().rev() {
                                stack.push(Task::VisitNode { node: stmt });
                            }
                            stack.push(Task::CaseStart { switch_node });
                        }
                    }
                }
                Task::CaseStart { switch_node } => {
                    let case_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(switch_node, case_start);
                    self.current_node = Some(case_start);
                }
                Task::CaseFinish => {
                    let idx = self.loop_stack.len() - 1;
                    let join = self.get_or_create_loop_break(idx);
                    if let Some(end) = self.current_node {
                        if end != self.cfg.exit {
                            self.cfg.add_edge(end, join);
                        }
                    }
                }
                Task::SwitchFinish { switch_node } => {
                    let join_node = self.get_or_create_loop_break(self.loop_stack.len() - 1);
                    self.loop_stack.pop();
                    self.cfg.add_edge(switch_node, join_node);
                    self.current_node = Some(join_node);
                }
            }
        }
    }

    fn dispatch_node<'a>(&mut self, node: Node<'a>, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "if_statement" => stack.push(Task::If { node }),
            "while_statement" | "for_statement" => stack.push(Task::Loop { node }),
            "do_statement" => stack.push(Task::DoWhile { node }),
            "switch_statement" => stack.push(Task::Switch { node }),
            "return_statement" => self.visit_return(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "goto_statement" => self.visit_goto(),
            "labeled_statement" => self.visit_labeled(node, stack),
            "compound_statement" => stack.push(Task::Block { block: node }),
            _ => self.visit_simple_statement(),
        }
    }

    /// The body of a loop or switch: the `body` field, else the first compound statement.
    fn loop_body<'a>(node: &Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_name("body")
            .or_else(|| find_child_by_kind(*node, "compound_statement"))
    }

    fn visit_simple_statement(&mut self) {
        if let Some(from_node) = self.current_node {
            let stmt_node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, stmt_node);
            self.current_node = Some(stmt_node);
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

    fn visit_labeled<'a>(&mut self, node: Node<'a>, stack: &mut Vec<Task<'a>>) {
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
        let body = named_children(&node)
            .into_iter()
            .find(|c| c.kind() != "statement_identifier");
        if let Some(body) = body {
            stack.push(Task::VisitNode { node: body });
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

fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.is_named())
        .collect()
}

fn children_of_kind<'a>(node: &Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() == kind)
        .collect()
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
                source: source.into(),
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
    fn test_if_else_both_return_no_orphan() {
        // Both branches return — join_node must not be created (would be unreachable).
        // Previously caused "Nodes not reachable from entry" CFG validation failure.
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
        assert!(cfg.node_count() >= 2, "CFG must be valid");
        assert_eq!(cc(source), 2);
    }

    #[test]
    fn test_chained_else_if_all_return_no_orphan() {
        // if / else-if / else all return — no join node, valid CFG.
        let source = r#"
int test_func(int x) {
    if (x > 0) {
        return 1;
    } else if (x < 0) {
        return -1;
    } else {
        return 0;
    }
}
"#;
        let function = make_c_function(source);
        let cfg = CCfgBuilder.build(&function);
        assert!(cfg.node_count() >= 2, "CFG must be valid");
        // 2 conditions → CC 3
        assert_eq!(cc(source), 3);
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

    /// Nesting this deep would overflow a recursive builder on a small stack.
    #[test]
    fn test_deeply_nested_if_else_does_not_overflow_and_builds_real_cfg() {
        const DEPTH: usize = 2_000;

        let mut source = String::from("void test(int x) {\n");
        for _ in 0..DEPTH {
            source.push_str("if (x > 0) {\n");
        }
        source.push_str("x = 1;\n");
        for _ in 0..DEPTH {
            source.push_str("} else {\nx = 2;\n}\n");
        }
        source.push_str("}\n");

        let node_count = std::thread::Builder::new()
            .stack_size(6 * 1024 * 1024)
            .spawn(move || {
                use crate::language::{CParser, LanguageParser};
                let parser = CParser::new().unwrap();
                let module = parser.parse(&source, "test.c").unwrap();
                let functions = module.discover_functions(0, &source);
                assert_eq!(functions.len(), 1);
                CCfgBuilder.build(&functions[0]).node_count()
            })
            .unwrap()
            .join()
            .unwrap();

        assert!(
            node_count > DEPTH,
            "expected a fully-built CFG with >{DEPTH} nodes, got {node_count}"
        );
    }
}
