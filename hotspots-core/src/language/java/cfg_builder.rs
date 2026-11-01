//! Java CFG builder implementation
//!
//! Walks the tree-sitter-java AST iteratively via an explicit heap-allocated
//! work stack (`Task`, driven by the `run` loop, with results threaded
//! through `self.current_node`) instead of native function-call recursion,
//! so nesting depth is bounded by heap memory rather than the thread stack
//! (https://github.com/Stephen-Collins-tech/hotspots/issues/160).

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use crate::language::tree_sitter_utils::{
    find_child_by_kind, find_function_by_start, with_cached_java_tree,
};
use tree_sitter::Node;

/// Java CFG builder
///
/// Builds control flow graphs from Java function bodies parsed with tree-sitter.
pub struct JavaCfgBuilder;

impl CfgBuilder for JavaCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_java();

        let result = with_cached_java_tree(source, |root| {
            let func_node = find_function_by_start(
                root,
                function.span.start,
                &["method_declaration", "constructor_declaration"],
            )?;
            let body_node = find_child_by_kind(func_node, "block")
                .or_else(|| find_child_by_kind(func_node, "constructor_body"))?;
            let mut builder = JavaCfgBuilderState::new();
            builder.build_from_block(body_node, source);
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
struct JavaCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    /// Stack of loop contexts for break/continue
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    break_target: NodeId,
    continue_target: NodeId,
}

/// Pending unit of work on the explicit stack.
///
/// The start variants (`If`, `While`, `Switch`, ...) do what the old
/// recursive `visit_*` methods did on the way down; the `*Finish`/`*After*`
/// variants do what they did with the nested result on the way back up. The
/// "result" of nested work is read from `self.current_node`, exactly as the
/// recursive methods used it.
enum Task<'a> {
    /// `build_from_block`: visit children, then connect the last node to exit.
    FromBlock {
        block: Node<'a>,
    },
    AfterFromBlock,
    /// `visit_block`: visit each named child in order.
    Block {
        block: Node<'a>,
    },
    VisitNode {
        node: Node<'a>,
    },
    Expr {
        node: Node<'a>,
    },
    If {
        node: Node<'a>,
    },
    IfAfterConsequence {
        alternative: Option<Node<'a>>,
        condition: NodeId,
        join: NodeId,
        had_consequence: bool,
    },
    IfFinish {
        join: NodeId,
    },
    /// Shared by `while` and traditional/enhanced `for`.
    Loop {
        node: Node<'a>,
    },
    LoopFinish {
        condition: NodeId,
        after_loop: NodeId,
        had_body: bool,
    },
    DoWhile {
        node: Node<'a>,
    },
    DoWhileFinish {
        body_entry: NodeId,
        condition: NodeId,
        after_loop: NodeId,
        had_body: bool,
    },
    Switch {
        node: Node<'a>,
    },
    SwitchLabel {
        switch_node: NodeId,
    },
    SwitchFinish {
        switch_node: NodeId,
        join: NodeId,
        has_default: bool,
        had_body: bool,
    },
    Try {
        node: Node<'a>,
    },
    /// Advance the try statement: record the end of the branch that just ran
    /// (if `collect`), then start the next catch clause or join the branches.
    TryNext {
        try_entry: NodeId,
        catches: Vec<Node<'a>>,
        idx: usize,
        branch_ends: Vec<NodeId>,
        collect: bool,
    },
    Synchronized {
        node: Node<'a>,
    },
    Ternary {
        node: Node<'a>,
    },
    TernaryAfterConsequence {
        alternative: Option<Node<'a>>,
        condition: NodeId,
        join: NodeId,
        had_consequence: bool,
    },
    TernaryFinish {
        join: NodeId,
    },
    ShortCircuit {
        node: Node<'a>,
    },
    ShortCircuitFinish {
        left_evaluated: NodeId,
        join: NodeId,
        had_right: bool,
    },
    Lambda {
        node: Node<'a>,
    },
}

impl JavaCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;

        JavaCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
        }
    }

    /// Build CFG from a block node
    fn build_from_block(&mut self, block: Node<'_>, source: &str) {
        let mut stack = vec![Task::FromBlock { block }];
        self.run(&mut stack, source);
    }

    /// Edge from the current node to `target`, unless there is no live path
    /// or the path already reached exit.
    fn connect_current_to(&mut self, target: NodeId) {
        if let Some(last) = self.current_node {
            if last != self.cfg.exit {
                self.cfg.add_edge(last, target);
            }
        }
    }

    /// Drive the explicit work-stack loop. Tasks are pushed in reverse so
    /// that they pop in source order.
    fn run<'a>(&mut self, stack: &mut Vec<Task<'a>>, source: &str) {
        while let Some(task) = stack.pop() {
            match task {
                Task::FromBlock { block } => {
                    stack.push(Task::AfterFromBlock);
                    stack.push(Task::Block { block });
                }
                Task::AfterFromBlock => {
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
                Task::Block { block } => {
                    for child in named_children(&block).into_iter().rev() {
                        stack.push(Task::VisitNode { node: child });
                    }
                }
                Task::VisitNode { node } => self.dispatch_node(node, source, stack),
                Task::Expr { node } => self.dispatch_expr(node, source, stack),
                Task::If { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let condition = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, condition);

                    let consequence = find_child_by_kind(node, "block");
                    let alternative = find_child_by_kind(node, "else_clause");

                    let join = self.cfg.add_node(NodeKind::Statement);

                    stack.push(Task::IfAfterConsequence {
                        alternative,
                        condition,
                        join,
                        had_consequence: consequence.is_some(),
                    });
                    if let Some(cons_block) = consequence {
                        self.current_node = Some(condition);
                        stack.push(Task::Block { block: cons_block });
                    }
                }
                Task::IfAfterConsequence {
                    alternative,
                    condition,
                    join,
                    had_consequence,
                } => {
                    if had_consequence {
                        self.connect_current_to(join);
                    } else {
                        self.cfg.add_edge(condition, join);
                    }

                    if let Some(alt_node) = alternative {
                        self.current_node = Some(condition);
                        stack.push(Task::IfFinish { join });

                        // `else if` is a nested if, a plain `else` is a block
                        for child in named_children(&alt_node).into_iter().rev() {
                            match child.kind() {
                                "if_statement" => stack.push(Task::If { node: child }),
                                "block" => stack.push(Task::Block { block: child }),
                                _ => {}
                            }
                        }
                    } else {
                        // No else clause, condition can go directly to join
                        self.cfg.add_edge(condition, join);
                        self.current_node = Some(join);
                    }
                }
                Task::IfFinish { join } => {
                    self.connect_current_to(join);
                    self.current_node = Some(join);
                }
                Task::Loop { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let condition = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, condition);

                    let after_loop = self.cfg.add_node(NodeKind::Statement);

                    self.loop_stack.push(LoopContext {
                        break_target: after_loop,
                        continue_target: condition,
                    });

                    let body = find_child_by_kind(node, "block");
                    stack.push(Task::LoopFinish {
                        condition,
                        after_loop,
                        had_body: body.is_some(),
                    });
                    if let Some(body) = body {
                        self.current_node = Some(condition);
                        stack.push(Task::Block { block: body });
                    }
                }
                Task::LoopFinish {
                    condition,
                    after_loop,
                    had_body,
                } => {
                    if had_body {
                        // Loop back to condition
                        self.connect_current_to(condition);
                    }

                    // Condition false edge to after loop
                    self.cfg.add_edge(condition, after_loop);

                    self.loop_stack.pop();
                    self.current_node = Some(after_loop);
                }
                Task::DoWhile { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let body_entry = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(current, body_entry);

                    let condition = self.cfg.add_node(NodeKind::Condition);
                    let after_loop = self.cfg.add_node(NodeKind::Statement);

                    self.loop_stack.push(LoopContext {
                        break_target: after_loop,
                        continue_target: condition,
                    });

                    let body = find_child_by_kind(node, "block");
                    stack.push(Task::DoWhileFinish {
                        body_entry,
                        condition,
                        after_loop,
                        had_body: body.is_some(),
                    });
                    if let Some(body) = body {
                        self.current_node = Some(body_entry);
                        stack.push(Task::Block { block: body });
                    }
                }
                Task::DoWhileFinish {
                    body_entry,
                    condition,
                    after_loop,
                    had_body,
                } => {
                    if had_body {
                        // Body goes to condition
                        self.connect_current_to(condition);
                    } else {
                        self.cfg.add_edge(body_entry, condition);
                    }

                    // Condition true edge loops back to body
                    self.cfg.add_edge(condition, body_entry);

                    // Condition false edge to after loop
                    self.cfg.add_edge(condition, after_loop);

                    self.loop_stack.pop();
                    self.current_node = Some(after_loop);
                }
                Task::Switch { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    // Switch node (decision point)
                    let switch_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, switch_node);

                    let join = self.cfg.add_node(NodeKind::Statement);

                    // Switch can be broken out of
                    self.loop_stack.push(LoopContext {
                        break_target: join,
                        continue_target: join, // continue not valid in switch, but use join
                    });

                    let switch_body = find_child_by_kind(node, "switch_block");
                    let children = switch_body
                        .map(|b| {
                            let mut cursor = b.walk();
                            b.children(&mut cursor).collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let has_default = children.iter().any(|c| {
                        c.kind() == "switch_label"
                            && source[c.start_byte()..c.end_byte()].contains("default")
                    });

                    stack.push(Task::SwitchFinish {
                        switch_node,
                        join,
                        has_default,
                        had_body: switch_body.is_some(),
                    });
                    for child in children.into_iter().rev() {
                        if child.kind() == "switch_label" {
                            stack.push(Task::SwitchLabel { switch_node });
                        } else if child.is_named() && child.kind() != "{" && child.kind() != "}" {
                            stack.push(Task::VisitNode { node: child });
                        }
                    }
                }
                Task::SwitchLabel { switch_node } => {
                    let case_node = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(switch_node, case_node);
                    self.current_node = Some(case_node);
                }
                Task::SwitchFinish {
                    switch_node,
                    join,
                    has_default,
                    had_body,
                } => {
                    if had_body {
                        // Connect last case to join (fall-through or explicit break)
                        self.connect_current_to(join);

                        // If no default case, switch can go directly to join
                        if !has_default {
                            self.cfg.add_edge(switch_node, join);
                        }
                    } else {
                        self.cfg.add_edge(switch_node, join);
                    }

                    self.loop_stack.pop();
                    self.current_node = Some(join);
                }
                Task::Try { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let try_entry = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(current, try_entry);

                    let catches: Vec<Node<'a>> = named_children(&node)
                        .into_iter()
                        .filter(|c| c.kind() == "catch_clause")
                        .collect();

                    let try_body = find_child_by_kind(node, "block");
                    // No try body: the entry node itself is the branch end
                    let branch_ends = if try_body.is_some() {
                        Vec::new()
                    } else {
                        vec![try_entry]
                    };
                    stack.push(Task::TryNext {
                        try_entry,
                        catches,
                        idx: 0,
                        branch_ends,
                        collect: try_body.is_some(),
                    });
                    if let Some(try_body) = try_body {
                        self.current_node = Some(try_entry);
                        stack.push(Task::Block { block: try_body });
                    }
                }
                Task::TryNext {
                    try_entry,
                    catches,
                    idx,
                    mut branch_ends,
                    collect,
                } => {
                    if collect {
                        if let Some(last) = self.current_node {
                            branch_ends.push(last);
                        }
                    }

                    if idx < catches.len() {
                        // Each catch is a separate branch from try entry
                        let catch_node = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(try_entry, catch_node);

                        let catch_body = find_child_by_kind(catches[idx], "block");
                        if catch_body.is_none() {
                            branch_ends.push(catch_node);
                        }
                        stack.push(Task::TryNext {
                            try_entry,
                            catches,
                            idx: idx + 1,
                            branch_ends,
                            collect: catch_body.is_some(),
                        });
                        if let Some(catch_body) = catch_body {
                            self.current_node = Some(catch_node);
                            stack.push(Task::Block { block: catch_body });
                        }
                    } else {
                        // Finally blocks are complex to model correctly in CFG because they
                        // execute on all exit paths. For simplicity, we skip modeling finally.

                        // Only create join node if there are branches that don't exit
                        let non_exit_branches: Vec<_> = branch_ends
                            .into_iter()
                            .filter(|&end| end != self.cfg.exit)
                            .collect();

                        if !non_exit_branches.is_empty() {
                            let join = self.cfg.add_node(NodeKind::Statement);
                            for end in non_exit_branches {
                                self.cfg.add_edge(end, join);
                            }
                            self.current_node = Some(join);
                        } else {
                            // All branches exit - current node is exit
                            self.current_node = Some(self.cfg.exit);
                        }
                    }
                }
                Task::Synchronized { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    // Decision point - acquiring lock
                    let sync_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, sync_node);

                    self.current_node = Some(sync_node);
                    if let Some(body) = find_child_by_kind(node, "block") {
                        stack.push(Task::Block { block: body });
                    }
                }
                Task::Ternary { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let condition = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, condition);

                    let join = self.cfg.add_node(NodeKind::Statement);

                    let consequence = node.child_by_field_name("consequence");
                    stack.push(Task::TernaryAfterConsequence {
                        alternative: node.child_by_field_name("alternative"),
                        condition,
                        join,
                        had_consequence: consequence.is_some(),
                    });
                    if let Some(consequence) = consequence {
                        self.current_node = Some(condition);
                        stack.push(Task::Expr { node: consequence });
                    }
                }
                Task::TernaryAfterConsequence {
                    alternative,
                    condition,
                    join,
                    had_consequence,
                } => {
                    if had_consequence {
                        self.connect_current_to(join);
                    } else {
                        self.cfg.add_edge(condition, join);
                    }

                    if let Some(alternative) = alternative {
                        self.current_node = Some(condition);
                        stack.push(Task::TernaryFinish { join });
                        stack.push(Task::Expr { node: alternative });
                    } else {
                        self.cfg.add_edge(condition, join);
                        self.current_node = Some(join);
                    }
                }
                Task::TernaryFinish { join } => {
                    self.connect_current_to(join);
                    self.current_node = Some(join);
                }
                Task::ShortCircuit { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let left_evaluated = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, left_evaluated);

                    let join = self.cfg.add_node(NodeKind::Statement);

                    // Right operand is evaluated only when short-circuiting doesn't apply.
                    let right = node.child_by_field_name("right");
                    stack.push(Task::ShortCircuitFinish {
                        left_evaluated,
                        join,
                        had_right: right.is_some(),
                    });
                    if let Some(right) = right {
                        self.current_node = Some(left_evaluated);
                        stack.push(Task::Expr { node: right });
                    } else {
                        self.cfg.add_edge(left_evaluated, join);
                    }
                }
                Task::ShortCircuitFinish {
                    left_evaluated,
                    join,
                    had_right,
                } => {
                    if had_right {
                        self.connect_current_to(join);
                    }

                    // Short-circuit path: right operand is skipped entirely.
                    self.cfg.add_edge(left_evaluated, join);

                    self.current_node = Some(join);
                }
                Task::Lambda { node } => {
                    self.visit_simple_statement();

                    if let Some(body) = node.child_by_field_name("body") {
                        if body.kind() == "block" {
                            stack.push(Task::Block { block: body });
                        } else {
                            stack.push(Task::Expr { node: body });
                        }
                    }
                }
            }
        }
    }

    /// Dispatch a tree-sitter node to the task (or inline handler) for its kind
    fn dispatch_node<'a>(&mut self, node: Node<'a>, source: &str, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "if_statement" => stack.push(Task::If { node }),
            "while_statement" => stack.push(Task::Loop { node }),
            "do_statement" => stack.push(Task::DoWhile { node }),
            "for_statement" | "enhanced_for_statement" => stack.push(Task::Loop { node }),
            "switch_statement" | "switch_expression" => stack.push(Task::Switch { node }),
            "try_statement" => stack.push(Task::Try { node }),
            "synchronized_statement" => stack.push(Task::Synchronized { node }),
            "return_statement" => self.visit_return(),
            "throw_statement" => self.visit_throw(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "expression_statement" => {
                // May contain ternary, &&, ||, lambdas
                if let Some(expr) = node.named_child(0) {
                    self.dispatch_expr(expr, source, stack);
                } else {
                    self.visit_simple_statement();
                }
            }
            // Local variable declarations, assertions, and anything else
            _ => self.visit_simple_statement(),
        }
    }

    /// Dispatch an expression to branch-aware handling for ternaries,
    /// &&/|| short-circuits, and lambdas with control flow.
    fn dispatch_expr<'a>(&mut self, node: Node<'a>, source: &str, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "ternary_expression" => stack.push(Task::Ternary { node }),
            "binary_expression" if Self::is_short_circuit_op(&node, source) => {
                stack.push(Task::ShortCircuit { node })
            }
            "lambda_expression" => stack.push(Task::Lambda { node }),
            "assignment_expression" => {
                if let Some(rhs) = node.child_by_field_name("right") {
                    stack.push(Task::Expr { node: rhs });
                } else {
                    self.visit_simple_statement();
                }
            }
            "method_invocation" => {
                let mut lambda_arg = None;
                if let Some(args) = node.child_by_field_name("arguments") {
                    let mut cursor = args.walk();
                    for child in args.named_children(&mut cursor) {
                        if child.kind() == "lambda_expression" {
                            lambda_arg = Some(child);
                            break;
                        }
                    }
                }

                if let Some(lambda) = lambda_arg {
                    stack.push(Task::Lambda { node: lambda });
                } else {
                    self.visit_simple_statement();
                }
            }
            _ => self.visit_simple_statement(),
        }
    }

    /// Returns true if the binary expression's operator is `&&` or `||`.
    fn is_short_circuit_op(node: &Node, source: &str) -> bool {
        node.child_by_field_name("operator")
            .map(|op| {
                let text = &source[op.start_byte()..op.end_byte()];
                text == "&&" || text == "||"
            })
            .unwrap_or(false)
    }

    /// Visit return statement
    fn visit_return(&mut self) {
        if let Some(current) = self.current_node {
            self.cfg.add_edge(current, self.cfg.exit);
            self.current_node = None; // Dead code after return
        }
    }

    /// Visit throw statement
    fn visit_throw(&mut self) {
        if let Some(current) = self.current_node {
            self.cfg.add_edge(current, self.cfg.exit);
            self.current_node = None; // Dead code after throw
        }
    }

    /// Visit break statement
    fn visit_break(&mut self) {
        if let Some(current) = self.current_node {
            if let Some(loop_ctx) = self.loop_stack.last() {
                self.cfg.add_edge(current, loop_ctx.break_target);
            } else {
                // Break outside loop - treat as going to exit
                self.cfg.add_edge(current, self.cfg.exit);
            }
            self.current_node = None; // Dead code after break
        }
    }

    /// Visit continue statement
    fn visit_continue(&mut self) {
        if let Some(current) = self.current_node {
            if let Some(loop_ctx) = self.loop_stack.last() {
                self.cfg.add_edge(current, loop_ctx.continue_target);
            } else {
                // Continue outside loop - treat as going to exit
                self.cfg.add_edge(current, self.cfg.exit);
            }
            self.current_node = None; // Dead code after continue
        }
    }

    /// Visit simple statement (no control flow impact)
    fn visit_simple_statement(&mut self) {
        if let Some(current) = self.current_node {
            let node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(current, node);
            self.current_node = Some(node);
        }
    }
}

fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.is_named())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    fn make_test_function(source: &str, start_byte: usize, end_byte: usize) -> FunctionNode {
        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test".to_string()),
            span: SourceSpan::new(start_byte, end_byte, 1, 1, 0),
            body: FunctionBody::Java {
                body_node: 0,
                source: source.into(),
            },
            suppression_reason: None,
        }
    }

    #[test]
    fn test_simple_method() {
        let source = r#"
public class Test {
    public void test() {
        int x = 1;
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 2); // At least entry and exit
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
public class Test {
    public void test(int x) {
        if (x > 0) {
            return;
        }
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3); // Entry, condition, exit
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
public class Test {
    public void test(int n) {
        int i = 0;
        while (i < n) {
            i++;
        }
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3); // Entry, condition, exit
    }

    #[test]
    fn test_for_loop() {
        let source = r#"
public class Test {
    public void test(int[] items) {
        for (int item : items) {
            System.out.println(item);
        }
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3);
    }

    #[test]
    fn test_try_catch() {
        let source = r#"
public class Test {
    public void test(int x) {
        try {
            int result = 10 / x;
        } catch (ArithmeticException e) {
            System.out.println("error");
        }
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 4); // Entry, try, catch, exit
    }

    #[test]
    fn test_ternary_expression() {
        let source = r#"
public class Test {
    public void test(int x) {
        int y;
        y = x > 0 ? 1 : -1;
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        // Entry, ternary condition, consequence, alternative, join, exit
        assert!(cfg.node_count() >= 5);
        assert!(
            cfg.edges.iter().filter(|e| e.from != cfg.entry).count() >= 4,
            "ternary should branch like an if/else"
        );
    }

    #[test]
    fn test_short_circuit_and() {
        let source = r#"
public class Test {
    public void test(Object a, Object b) {
        boolean ok;
        ok = a != null && b.equals(a);
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        // Entry, left-evaluated condition, right operand, join, exit
        assert!(cfg.node_count() >= 4);
    }

    #[test]
    fn test_short_circuit_or() {
        let source = r#"
public class Test {
    public void test(boolean a, boolean b) {
        boolean ok;
        ok = a || b;
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 4);
    }

    #[test]
    fn test_lambda_with_control_flow() {
        let source = r#"
public class Test {
    public void test(java.util.List<Integer> items) {
        items.forEach(item -> {
            if (item > 0) {
                System.out.println(item);
            }
        });
    }
}
"#;
        let function = make_test_function(
            source,
            source.find("public void test").unwrap(),
            source.len(),
        );
        let builder = JavaCfgBuilder;
        let cfg = builder.build(&function);

        cfg.validate().unwrap();
        // Entry, call-site node, lambda's if-condition, then-branch, join, exit
        assert!(cfg.node_count() >= 5);
    }

    /// Nesting this deep would overflow a recursive builder on a small stack.
    #[test]
    fn test_deeply_nested_if_else_does_not_overflow_and_builds_real_cfg() {
        const DEPTH: usize = 2_000;

        let mut source = String::from("class T {\nvoid test(int x) {\n");
        for _ in 0..DEPTH {
            source.push_str("if (x > 0) {\n");
        }
        source.push_str("x = 1;\n");
        for _ in 0..DEPTH {
            source.push_str("} else {\nx = 2;\n}\n");
        }
        source.push_str("}\n}\n");

        let node_count = std::thread::Builder::new()
            .stack_size(6 * 1024 * 1024)
            .spawn(move || {
                use crate::language::{JavaParser, LanguageParser};
                let parser = JavaParser::new().unwrap();
                let module = parser.parse(&source, "T.java").unwrap();
                let functions = module.discover_functions(0, &source);
                assert_eq!(functions.len(), 1);
                JavaCfgBuilder.build(&functions[0]).node_count()
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
