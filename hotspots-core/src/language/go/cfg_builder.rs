//! Go CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use crate::language::tree_sitter_utils::{
    find_child_by_kind, find_function_by_start, with_cached_go_tree,
};
use tree_sitter::Node;

/// Go CFG builder
///
/// Builds control flow graphs from Go function bodies parsed with tree-sitter.
pub struct GoCfgBuilder;

impl CfgBuilder for GoCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_go();

        let result = with_cached_go_tree(source, |root| {
            let func_node = find_function_by_start(
                root,
                function.span.start,
                &["function_declaration", "method_declaration"],
            )?;
            let body_node = find_child_by_kind(func_node, "block")?;
            let mut builder = GoCfgBuilderState::new();
            builder.build_from_block(&body_node, source);
            Some(builder.cfg)
        });

        result.unwrap_or_else(|| {
            let mut cfg = Cfg::new();
            cfg.add_edge(cfg.entry, cfg.exit);
            cfg
        })
    }
}

/// Internal builder state for constructing the CFG
struct GoCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    /// Stack of loop/switch/select contexts for break/continue
    loop_stack: Vec<LoopContext>,
    /// Label attached to the next loop/switch/select statement visited, if any
    pending_label: Option<String>,
}

struct LoopContext {
    /// Lazily created: `None` until something actually needs to jump here
    /// (a `break`, a case falling through, or a no-default fallback edge).
    /// A `for` loop's own header->join "exit" edge always creates it eagerly
    /// since that path is unconditional, so it's only ever lazy for
    /// switch/select, where every case may terminate control flow itself
    /// (e.g. `return` in every case), leaving the join node unneeded.
    break_target: Option<NodeId>,
    /// `None` for switch/select contexts, which do not accept `continue`
    continue_target: Option<NodeId>,
    /// Label naming this construct, for labeled break/continue
    label: Option<String>,
}

impl GoCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;

        GoCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
            pending_label: None,
        }
    }

    /// Get or lazily create the break target for the loop/switch/select
    /// context at `idx` in `loop_stack`.
    fn get_or_create_break_target(&mut self, idx: usize) -> NodeId {
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
            "break_statement" => self.visit_break(node, source),
            "continue_statement" => self.visit_continue(node, source),
            "goto_statement" => self.visit_goto(),
            "labeled_statement" => self.visit_labeled(node, source),
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
            "statement_list" => {
                // tree-sitter-go wraps every sequence of statements (a
                // function/if/for block body, a switch/select case body,
                // etc.) in a statement_list node. Without flattening it
                // here, every statement inside would be treated as one
                // opaque, un-dispatched node - collapsing entire multi-
                // statement blocks into a single generic statement and
                // silently skipping any if/for/switch/etc. nested inside.
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.is_named() {
                        self.visit_node(&child, source);
                    }
                }
            }
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

        // Push loop context for break/continue. The join node is created
        // lazily below via get_or_create_break_target, but a `for` loop's
        // header always has an unconditional "exit" edge to it (the loop
        // condition can always be false, or there is none), so it will
        // always end up created.
        let label = self.pending_label.take();
        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: Some(loop_header),
            label,
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

        // Exit condition from loop header
        let idx = self.loop_stack.len() - 1;
        let join_node = self.get_or_create_break_target(idx);
        self.cfg.add_edge(loop_header, join_node);

        // Pop loop context
        self.loop_stack.pop();

        self.current_node = Some(join_node);
    }

    fn visit_switch(&mut self, node: &Node, source: &str) {
        let from_node = self.current_node.expect("Current node should exist");

        // Switch condition
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);

        // Push break context (switch/select break independently of any
        // enclosing loop; `continue` is not valid here, so continue_target
        // is None and unlabeled continue skips over this context). The join
        // node itself is created lazily: if every case terminates control
        // flow (e.g. `return` in every case) and there's a default (so no
        // fallback-to-join edge is added either), nothing ever needs a join
        // node, and creating one eagerly would leave it unreachable.
        let idx = self.loop_stack.len();
        let label = self.pending_label.take();
        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: None,
            label,
        });

        // Visit each case. Regular switches use "expression_case"; type
        // switches (switch x.(type) { case T: ... }) use "type_case".
        let mut has_default = false;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "expression_case"
                || child.kind() == "type_case"
                || child.kind() == "default_case"
            {
                if child.kind() == "default_case" {
                    has_default = true;
                }

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
                        let join_node = self.get_or_create_break_target(idx);
                        self.cfg.add_edge(case_end, join_node);
                    }
                }
            }
        }

        // If there's no default case, no case is guaranteed to match, so the
        // switch can fall through directly to join. When a default exists,
        // one case always executes, so this edge would be spurious.
        if !has_default {
            let join_node = self.get_or_create_break_target(idx);
            self.cfg.add_edge(condition_node, join_node);
        }

        let ctx = self.loop_stack.pop().expect("pushed switch context above");
        self.current_node = ctx.break_target;
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

        // Push break context (select break independently of any enclosing
        // loop; `continue` is not valid here). The join node is created
        // lazily: if every case terminates control flow (e.g. `return` in
        // every case), nothing ever needs a join node, and creating one
        // eagerly would leave it unreachable.
        let idx = self.loop_stack.len();
        let label = self.pending_label.take();
        self.loop_stack.push(LoopContext {
            break_target: None,
            continue_target: None,
            label,
        });

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
                        let join_node = self.get_or_create_break_target(idx);
                        self.cfg.add_edge(case_end, join_node);
                    }
                }
            }
        }

        let ctx = self.loop_stack.pop().expect("pushed select context above");
        self.current_node = ctx.break_target;
    }

    fn visit_return(&mut self, _node: &Node) {
        if let Some(from_node) = self.current_node {
            let return_node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, return_node);
            self.cfg.add_edge(return_node, self.cfg.exit);
            self.current_node = None; // Dead code after return
        }
    }

    fn visit_break(&mut self, node: &Node, source: &str) {
        if let Some(from_node) = self.current_node {
            let label = find_label_text(node, source);
            let idx = match &label {
                Some(label) => self
                    .loop_stack
                    .iter()
                    .rposition(|ctx| ctx.label.as_deref() == Some(label.as_str())),
                None => {
                    if self.loop_stack.is_empty() {
                        None
                    } else {
                        Some(self.loop_stack.len() - 1)
                    }
                }
            };

            if let Some(idx) = idx {
                let target = self.get_or_create_break_target(idx);
                let break_node = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(from_node, break_node);
                self.cfg.add_edge(break_node, target);
                self.current_node = None; // Dead code after break
            }
        }
    }

    fn visit_continue(&mut self, node: &Node, source: &str) {
        if let Some(from_node) = self.current_node {
            let label = find_label_text(node, source);
            // `continue` only targets an enclosing `for` loop; switch/select
            // contexts (continue_target: None) are transparent to it.
            let target = match &label {
                Some(label) => self
                    .loop_stack
                    .iter()
                    .rev()
                    .find(|ctx| ctx.label.as_deref() == Some(label.as_str()))
                    .and_then(|ctx| ctx.continue_target),
                None => self
                    .loop_stack
                    .iter()
                    .rev()
                    .find_map(|ctx| ctx.continue_target),
            };

            if let Some(target) = target {
                let continue_node = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(from_node, continue_node);
                self.cfg.add_edge(continue_node, target);
                self.current_node = None; // Dead code after continue
            }
        }
    }

    fn visit_goto(&mut self) {
        if let Some(from_node) = self.current_node {
            // goto is a branch: +1 CC, then control leaves this path.
            // We don't track named jump targets (matches the C builder's
            // approximation), so it is modeled as an edge straight to exit.
            let goto_node = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(from_node, goto_node);
            self.cfg.add_edge(goto_node, self.cfg.exit);
            self.current_node = None;
        }
    }

    fn visit_labeled(&mut self, node: &Node, source: &str) {
        // A label is a jump target reachable via goto. Always create a node
        // for it so dead code after a goto/return/break doesn't panic.
        let label_node = self.cfg.add_node(NodeKind::Statement);
        if let Some(from) = self.current_node {
            self.cfg.add_edge(from, label_node);
        } else {
            self.cfg.add_edge(self.cfg.entry, label_node);
        }
        self.current_node = Some(label_node);

        // Record the label so the following loop/switch/select statement
        // can be targeted by a labeled break/continue.
        self.pending_label = find_label_text(node, source);

        // Visit the labeled statement itself (skip the label_name child)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_named() && child.kind() != "label_name" {
                self.visit_node(&child, source);
                break;
            }
        }

        self.pending_label = None;
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

/// Find a child node by field name
fn find_child_by_field<'a>(node: Node<'a>, field: &str) -> Option<Node<'a>> {
    node.child_by_field_name(field)
}

/// Extract the label name text from a labeled/break/continue statement node,
/// if it has a `label_name` child.
fn find_label_text(node: &Node, source: &str) -> Option<String> {
    find_child_by_kind(*node, "label_name")
        .map(|n| source[n.start_byte()..n.end_byte()].to_string())
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

    /// Parse `source`, find the first declared function, and build its CFG.
    fn build_cfg(source: &str) -> Cfg {
        use crate::language::GoParser;
        let parser = GoParser::new().unwrap();
        let module = parser.parse(source, "test.go").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 1, "expected exactly one function");
        let builder = GoCfgBuilder;
        builder.build(&functions[0])
    }

    // ---- go statements (goroutines) ----

    #[test]
    fn test_go_statement_simple() {
        let source = r#"
package main
func test() {
    go doWork()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // go statement is a fork, not a branch: entry -> stmt -> exit, no CC increase
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_go_statement_with_closure_body_not_walked() {
        // The goroutine body (containing an if) must not be walked into as
        // part of the enclosing function's control flow - it executes
        // concurrently, not inline.
        let source = r#"
package main
func test(x int) {
    go func() {
        if x > 0 {
            doWork()
        }
    }()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // No Condition node should be produced from the closure's inner `if`,
        // since the goroutine body is not part of this function's CFG.
        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert_eq!(
            condition_count, 0,
            "goroutine body should not be walked into"
        );
    }

    #[test]
    fn test_multiple_go_statements_sequential() {
        let source = r#"
package main
func test() {
    go doWork()
    go doOtherWork()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3);
    }

    // ---- defer statements ----

    #[test]
    fn test_defer_statement_simple() {
        let source = r#"
package main
func test() {
    defer cleanup()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // defer does not fork or branch: no Condition node produced
        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert_eq!(condition_count, 0);
    }

    #[test]
    fn test_defer_does_not_execute_inline() {
        // A defer followed by more statements should behave as ordinary
        // sequential flow: defer doesn't jump or terminate the block.
        let source = r#"
package main
func test() {
    defer cleanup()
    doWork()
    doMore()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // entry -> defer_stmt -> doWork -> doMore -> exit: linear chain
        assert_eq!(cfg.node_count(), 5);
        assert_eq!(cfg.edge_count(), 4);
    }

    #[test]
    fn test_multiple_defers() {
        let source = r#"
package main
func test() {
    defer cleanup()
    defer cleanup()
    defer cleanup()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 4);
    }

    // ---- select statements ----

    #[test]
    fn test_select_statement_branches_like_switch() {
        let source = r#"
package main
func test(ch1 chan int, ch2 chan int) {
    select {
    case <-ch1:
        doA()
    case <-ch2:
        doB()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // Each communication_case should be reachable from a shared
        // Condition node (the select), and both should reach a join.
        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert_eq!(condition_count, 1, "select should add one Condition node");
    }

    #[test]
    fn test_select_with_default() {
        let source = r#"
package main
func test(ch chan int) {
    select {
    case v := <-ch:
        _ = v
    default:
        doB()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 4);
    }

    #[test]
    fn test_select_in_loop_break_targets_select_not_loop() {
        // An unlabeled `break` inside a select's case must exit the select,
        // not the enclosing `for` loop.
        let source = r#"
package main
func test(ch chan int) {
    for {
        select {
        case <-ch:
            break
        default:
            doB()
        }
        doAfterSelect()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // doAfterSelect() must still be reachable (break only exits the
        // select, so the loop body continues past it).
        let stmt_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Statement))
            .count();
        // entry-adjacent statements: select cases (2) + break + doB + doAfterSelect >= 4
        assert!(stmt_count >= 4, "got {} statement nodes", stmt_count);
    }

    // ---- type switches ----

    #[test]
    fn test_type_switch_visits_every_case() {
        // Regression: type_case nodes (used by `switch x.(type)`) were
        // previously not recognized by the switch visitor (only
        // expression_case/default_case were), so non-default cases were
        // silently skipped rather than contributing to the CFG.
        let source = r#"
package main
func test(x interface{}) {
    switch x.(type) {
    case int:
        doInt()
    case string:
        doString()
    default:
        doOther()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        let stmt_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Statement))
            .count();
        // At least 3 case-start nodes + 3 case-body statement nodes = 6.
        // (Before the fix, only the default_case was recognized, so the
        // int/string type_case branches were silently dropped.)
        assert!(
            stmt_count >= 6,
            "all three type_case/default_case branches should be visited, got {} statement nodes",
            stmt_count
        );
    }

    #[test]
    fn test_type_switch_without_default_has_fallthrough_edge() {
        let source = r#"
package main
func test(x interface{}) {
    switch x.(type) {
    case int:
        doInt()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        // No default: the switch condition must have a direct edge to join
        // representing "no case matched".
        assert!(cfg.node_count() >= 4);
    }

    // ---- switch default-case fallthrough edge ----

    #[test]
    fn test_switch_with_default_has_no_spurious_fallthrough_edge() {
        // Regression: the switch visitor used to unconditionally add a
        // condition -> join edge even when a default case exists, even
        // though a default case guarantees some case always executes.
        let with_default = r#"
package main
func test(x int) {
    switch x {
    case 1:
        doA()
    default:
        doB()
    }
}
"#;
        let without_default = r#"
package main
func test(x int) {
    switch x {
    case 1:
        doA()
    }
}
"#;
        let cfg_with = build_cfg(with_default);
        let cfg_without = build_cfg(without_default);
        cfg_with.validate().unwrap();
        cfg_without.validate().unwrap();

        assert!(
            !has_condition_to_join_edge(&cfg_with),
            "switch with a default case must not have a direct \
             condition -> join fallthrough edge, since a case always matches"
        );
        assert!(
            has_condition_to_join_edge(&cfg_without),
            "switch without a default case must have a direct \
             condition -> join fallthrough edge for the no-match path"
        );
    }

    /// True if the CFG has an edge directly from a Condition node to a Join node.
    fn has_condition_to_join_edge(cfg: &Cfg) -> bool {
        let kind_of = |id: NodeId| cfg.nodes.iter().find(|n| n.id == id).map(|n| n.kind);
        cfg.edges.iter().any(|e| {
            matches!(kind_of(e.from), Some(NodeKind::Condition))
                && matches!(kind_of(e.to), Some(NodeKind::Join))
        })
    }

    // ---- labeled break/continue ----

    #[test]
    fn test_labeled_continue_targets_outer_loop() {
        let source = r#"
package main
func test() {
Outer:
    for i := 0; i < 10; i++ {
        for j := 0; j < 10; j++ {
            if j == 5 {
                continue Outer
            }
        }
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 5);
    }

    #[test]
    fn test_labeled_break_targets_outer_loop() {
        let source = r#"
package main
func test() {
Outer:
    for i := 0; i < 10; i++ {
        for j := 0; j < 10; j++ {
            if j == 5 {
                break Outer
            }
        }
        doAfterOuterLoop()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 6);
    }

    #[test]
    fn test_labeled_break_from_switch_in_loop() {
        let source = r#"
package main
func test(x int) {
Loop:
    for i := 0; i < 10; i++ {
        switch x {
        case 1:
            break Loop
        default:
            doB()
        }
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 5);
    }

    #[test]
    fn test_unlabeled_break_in_switch_does_not_break_outer_loop() {
        // Regression: switch/select never pushed their own break context,
        // so an unlabeled `break` inside a switch nested in a loop was
        // silently dropped instead of exiting only the switch.
        let source = r#"
package main
func test(x int) {
    for i := 0; i < 10; i++ {
        switch x {
        case 1:
            break
        default:
            doB()
        }
        doAfterSwitch()
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        let stmt_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Statement))
            .count();
        // doAfterSwitch() must be present and reachable in the CFG.
        assert!(stmt_count >= 5, "got {} statement nodes", stmt_count);
    }

    // ---- goto / labeled statements ----

    #[test]
    fn test_goto_statement_terminates_current_path() {
        let source = r#"
package main
func test(x int) {
    if x < 0 {
        goto Done
    }
    doWork()
Done:
    return
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        // if-condition + goto-as-branch = 2 Condition nodes
        assert_eq!(condition_count, 2);
    }

    #[test]
    fn test_labeled_statement_wrapping_loop_is_walked() {
        // Regression: labeled_statement previously matched the catch-all
        // branch, which added a single generic Statement node instead of
        // descending into the labeled loop - completely dropping the
        // loop's own control flow from the CFG.
        let source = r#"
package main
func test() {
Loop:
    for i := 0; i < 10; i++ {
        if i == 5 {
            break Loop
        }
    }
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert!(
            condition_count >= 1,
            "labeled for-loop's inner `if` must still be walked, got {} Condition nodes",
            condition_count
        );
    }

    #[test]
    fn test_label_after_goto_does_not_panic() {
        // Dead code (after goto) reaching a label must not panic when
        // building the CFG.
        let source = r#"
package main
func test(x int) {
    goto Done
    doUnreachable()
Done:
    doWork()
}
"#;
        let cfg = build_cfg(source);
        cfg.validate().unwrap();
    }
}
