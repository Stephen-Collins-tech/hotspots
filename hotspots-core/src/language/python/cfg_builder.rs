//! Python CFG builder implementation
//!
//! Walks the tree-sitter-python AST iteratively via an explicit heap-allocated
//! work stack (`Task`, driven by the `run` loop, with results threaded
//! through `self.current_node`) instead of native function-call recursion,
//! so nesting depth is bounded by heap memory rather than the thread stack
//! (https://github.com/Stephen-Collins-tech/hotspots/issues/160).

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use crate::language::tree_sitter_utils::{
    find_child_by_kind, find_function_by_start, with_cached_python_tree,
};
use tree_sitter::Node;

/// Python CFG builder
///
/// Builds control flow graphs from Python function bodies parsed with tree-sitter.
pub struct PythonCfgBuilder;

impl CfgBuilder for PythonCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_python();

        let result = with_cached_python_tree(source, |root| {
            let func_node = find_function_by_start(
                root,
                function.span.start,
                &["function_definition", "async_function_definition"],
            )?;
            let body_node = find_child_by_kind(func_node, "block")?;
            let mut builder = PythonCfgBuilderState::new();
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

/// Pending unit of work on the explicit stack.
///
/// The start variants (`If`, `Loop`, `Try`, `Match`) do what the old
/// recursive `visit_*` methods did on the way down; the `*Chain`/`*Finish`/
/// `*After*` variants do what they did with the nested result on the way back
/// up. The "result" of nested work is read from `self.current_node`, exactly
/// as the recursive methods used it.
enum Task<'a> {
    /// Function's top-level body: walk statements, then wire the tail to exit.
    FromBlock {
        block: Node<'a>,
    },
    AfterFromBlock,
    /// `build_block_statements`: visit each named child in order, without
    /// wiring the tail to the function exit.
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
        node: Node<'a>,
        condition_node: NodeId,
        then_start: NodeId,
    },
    /// Walk the `elif`/`else` clauses of an `if`, one at a time.
    IfChain {
        clauses: Vec<Node<'a>>,
        idx: usize,
        join_node: NodeId,
        last_condition: NodeId,
        branch_ends: Vec<NodeId>,
        has_else: bool,
        /// Start node of the branch whose block just ran
        pending: Option<NodeId>,
    },
    /// Shared by `while` and `for`.
    Loop {
        node: Node<'a>,
    },
    LoopFinish {
        loop_header: NodeId,
        join_node: NodeId,
        had_body: bool,
    },
    Try {
        node: Node<'a>,
    },
    TryAfterBody {
        node: Node<'a>,
        from_node: NodeId,
        try_start: NodeId,
    },
    /// Walk the `except`/`else`/`finally` clauses of a `try`, one at a time.
    TryChain {
        clauses: Vec<Node<'a>>,
        idx: usize,
        from_node: NodeId,
        try_start: NodeId,
        try_end: NodeId,
        branch_ends: Vec<NodeId>,
        /// Start node of the clause whose block just ran, and whether its end
        /// replaces (`finally`) rather than extends the branch ends
        pending: Option<(NodeId, bool)>,
    },
    Match {
        node: Node<'a>,
    },
    /// Walk the `case` clauses of a `match`, one at a time.
    MatchChain {
        cases: Vec<Node<'a>>,
        idx: usize,
        join_node: NodeId,
        last_condition: NodeId,
        branch_ends: Vec<NodeId>,
        pending: Option<NodeId>,
    },
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

    /// Build CFG from the *function's top-level* body block: walks its
    /// statements, then wires the last node to the function's exit if nothing
    /// already routes there. Only call this for the outermost function body —
    /// nested blocks (if/elif/else/while/for/try/match bodies) are walked via
    /// [`Task::Block`] instead, which skips the exit-wiring step. Wiring every
    /// nested block's tail to the function exit directly (instead of to that
    /// construct's own join/loop-header node) added a spurious extra edge per
    /// branch, inflating cyclomatic complexity by +1 per branch regardless of
    /// whether the branch actually returned early — see hotspots#230.
    fn build_from_block(&mut self, block: Node<'_>, source: &str) {
        let mut stack = vec![Task::FromBlock { block }];
        self.run(&mut stack, source);
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
                    // Skip structural nodes, process only named children
                    for child in named_children(&block).into_iter().rev() {
                        stack.push(Task::VisitNode { node: child });
                    }
                }
                Task::VisitNode { node } => self.dispatch_node(node, source, stack),
                Task::If { node } => {
                    let from_node = self.current_node.expect("Current node should exist");

                    // Condition node
                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    // Then branch (consequence)
                    let then_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, then_start);

                    stack.push(Task::IfAfterThen {
                        node,
                        condition_node,
                        then_start,
                    });
                    if let Some(consequence) = find_child_by_kind(node, "block") {
                        self.current_node = Some(then_start);
                        stack.push(Task::Block { block: consequence });
                    }
                }
                Task::IfAfterThen {
                    node,
                    condition_node,
                    then_start,
                } => {
                    let then_end = self.current_node.unwrap_or(then_start);

                    // Handle elif_clause and else_clause
                    let join_node = self.cfg.add_node(NodeKind::Join);
                    let clauses = children_of_kinds(&node, &["elif_clause", "else_clause"]);
                    stack.push(Task::IfChain {
                        clauses,
                        idx: 0,
                        join_node,
                        last_condition: condition_node,
                        branch_ends: vec![then_end],
                        has_else: false,
                        pending: None,
                    });
                }
                Task::IfChain {
                    clauses,
                    idx,
                    join_node,
                    mut last_condition,
                    mut branch_ends,
                    mut has_else,
                    pending,
                } => {
                    if let Some(start) = pending {
                        branch_ends.push(self.current_node.unwrap_or(start));
                    }

                    let Some(&child) = clauses.get(idx) else {
                        // If no else clause, the last condition can fall through directly to
                        // join. When an else branch exists, that path is already covered by
                        // the condition->else_start edge, so adding this would be a spurious
                        // extra edge.
                        if !has_else {
                            self.cfg.add_edge(last_condition, join_node);
                        }

                        // Connect all branch ends to join
                        for end in branch_ends {
                            if end != self.cfg.exit {
                                self.cfg.add_edge(end, join_node);
                            }
                        }

                        self.current_node = Some(join_node);
                        continue;
                    };

                    let mut body_start = None;
                    let mut body = None;
                    if child.kind() == "elif_clause" {
                        // Create condition node for elif
                        let elif_condition = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(last_condition, elif_condition);

                        if let Some(elif_body) = find_child_by_kind(child, "block") {
                            let elif_start = self.cfg.add_node(NodeKind::Statement);
                            self.cfg.add_edge(elif_condition, elif_start);
                            self.current_node = Some(elif_start);
                            body_start = Some(elif_start);
                            body = Some(elif_body);
                        }

                        last_condition = elif_condition;
                    } else {
                        has_else = true;
                        if let Some(else_body) = find_child_by_kind(child, "block") {
                            let else_start = self.cfg.add_node(NodeKind::Statement);
                            self.cfg.add_edge(last_condition, else_start);
                            self.current_node = Some(else_start);
                            body_start = Some(else_start);
                            body = Some(else_body);
                        }
                    }

                    stack.push(Task::IfChain {
                        clauses,
                        idx: idx + 1,
                        join_node,
                        last_condition,
                        branch_ends,
                        has_else,
                        pending: body_start,
                    });
                    if let Some(block) = body {
                        stack.push(Task::Block { block });
                    }
                }
                Task::Loop { node } => {
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

                    let body = find_child_by_kind(node, "block");
                    stack.push(Task::LoopFinish {
                        loop_header,
                        join_node,
                        had_body: body.is_some(),
                    });
                    if let Some(body) = body {
                        self.current_node = Some(body_start);
                        stack.push(Task::Block { block: body });
                    }
                }
                Task::LoopFinish {
                    loop_header,
                    join_node,
                    had_body,
                } => {
                    if had_body {
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
                Task::Try { node } => {
                    let from_node = self.current_node.expect("Current node should exist");

                    let try_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(from_node, try_start);

                    stack.push(Task::TryAfterBody {
                        node,
                        from_node,
                        try_start,
                    });
                    if let Some(body) = find_child_by_kind(node, "block") {
                        self.current_node = Some(try_start);
                        stack.push(Task::Block { block: body });
                    }
                }
                Task::TryAfterBody {
                    node,
                    from_node,
                    try_start,
                } => {
                    let try_end = self.current_node.unwrap_or(try_start);
                    let clauses = children_of_kinds(
                        &node,
                        &["except_clause", "else_clause", "finally_clause"],
                    );
                    stack.push(Task::TryChain {
                        clauses,
                        idx: 0,
                        from_node,
                        try_start,
                        try_end,
                        branch_ends: vec![try_end],
                        pending: None,
                    });
                }
                Task::TryChain {
                    clauses,
                    idx,
                    from_node,
                    try_start,
                    try_end,
                    mut branch_ends,
                    pending,
                } => {
                    if let Some((start, replace)) = pending {
                        let end = self.current_node.unwrap_or(start);
                        if replace {
                            branch_ends = vec![end];
                        } else {
                            branch_ends.push(end);
                        }
                    }

                    let Some(&child) = clauses.get(idx) else {
                        self.finish_try(branch_ends);
                        continue;
                    };

                    let mut next_pending = None;
                    let mut body = None;
                    match child.kind() {
                        "except_clause" => {
                            let except_condition = self.cfg.add_node(NodeKind::Condition);
                            self.cfg.add_edge(from_node, except_condition);
                            let except_start = self.cfg.add_node(NodeKind::Statement);
                            self.cfg.add_edge(except_condition, except_start);
                            self.current_node = Some(except_start);
                            body = find_child_by_kind(child, "block");
                            next_pending = Some((except_start, false));
                        }
                        "else_clause" => {
                            if let Some(else_body) = find_child_by_kind(child, "block") {
                                let else_start = self.cfg.add_node(NodeKind::Statement);
                                self.cfg.add_edge(try_end, else_start);
                                self.current_node = Some(else_start);
                                body = Some(else_body);
                                next_pending = Some((else_start, false));
                            }
                        }
                        _ => {
                            let finally_node = self.cfg.add_node(NodeKind::Statement);
                            let exit = self.cfg.exit;
                            let mut connected = false;
                            for &end in &branch_ends {
                                if end != exit {
                                    self.cfg.add_edge(end, finally_node);
                                    connected = true;
                                }
                            }
                            // When all branches terminate (return/raise), finally_node would
                            // have no predecessors. Fall back to try_start so finally remains
                            // reachable, matching the JS builder's connect_finally fallback.
                            if !connected {
                                self.cfg.add_edge(try_start, finally_node);
                            }
                            body = find_child_by_kind(child, "block");
                            if body.is_some() {
                                self.current_node = Some(finally_node);
                            }
                            next_pending = Some((finally_node, true));
                        }
                    }

                    stack.push(Task::TryChain {
                        clauses,
                        idx: idx + 1,
                        from_node,
                        try_start,
                        try_end,
                        branch_ends,
                        pending: next_pending,
                    });
                    if let Some(block) = body {
                        stack.push(Task::Block { block });
                    }
                }
                Task::Match { node } => {
                    let from_node = self.current_node.expect("Current node should exist");

                    // The match body is a `block` containing `case_clause` children.
                    let Some(match_body) = find_child_by_kind(node, "block") else {
                        self.visit_simple_statement();
                        continue;
                    };

                    let cases = children_of_kinds(&match_body, &["case_clause"]);
                    if cases.is_empty() {
                        self.visit_simple_statement();
                        continue;
                    }

                    // Model each `case` clause as its own branch off the match subject,
                    // chained like an if/elif chain, rejoining at a shared join node.
                    let join_node = self.cfg.add_node(NodeKind::Join);
                    stack.push(Task::MatchChain {
                        cases,
                        idx: 0,
                        join_node,
                        last_condition: from_node,
                        branch_ends: Vec::new(),
                        pending: None,
                    });
                }
                Task::MatchChain {
                    cases,
                    idx,
                    join_node,
                    mut last_condition,
                    mut branch_ends,
                    pending,
                } => {
                    if let Some(start) = pending {
                        branch_ends.push(self.current_node.unwrap_or(start));
                    }

                    let Some(&case) = cases.get(idx) else {
                        // As with if/elif chains, the last condition in the chain always
                        // links to join so join remains reachable even when every case body
                        // terminates (return/raise/break).
                        self.cfg.add_edge(last_condition, join_node);

                        // Connect all branch ends to join
                        for end in branch_ends {
                            if end != self.cfg.exit {
                                self.cfg.add_edge(end, join_node);
                            }
                        }

                        self.current_node = Some(join_node);
                        continue;
                    };

                    let consequence = find_child_by_kind(case, "block");
                    let mut next_idx = idx + 1;
                    let mut case_start = None;

                    if is_wildcard_case(&case, source) {
                        // `case _:` with no guard always matches once reached, so it
                        // behaves like an `else` clause: no further condition needed,
                        // and it is the last case considered (matching Python's
                        // fall-through semantics for an unconditional wildcard).
                        next_idx = cases.len();
                        if consequence.is_some() {
                            let start = self.cfg.add_node(NodeKind::Statement);
                            self.cfg.add_edge(last_condition, start);
                            self.current_node = Some(start);
                            case_start = Some(start);
                        }
                    } else {
                        let case_condition = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(last_condition, case_condition);

                        if consequence.is_some() {
                            let start = self.cfg.add_node(NodeKind::Statement);
                            self.cfg.add_edge(case_condition, start);
                            self.current_node = Some(start);
                            case_start = Some(start);
                        }

                        last_condition = case_condition;
                    }

                    stack.push(Task::MatchChain {
                        cases,
                        idx: next_idx,
                        join_node,
                        last_condition,
                        branch_ends,
                        pending: case_start,
                    });
                    if let Some(block) = consequence {
                        stack.push(Task::Block { block });
                    }
                }
            }
        }
    }

    /// Join the surviving `try` branches (or route to exit if none survive).
    fn finish_try(&mut self, branch_ends: Vec<NodeId>) {
        // A branch ending in `break`/`continue` has already been routed to the
        // enclosing loop's join/header by `visit_break`/`visit_continue` — like
        // a branch ending in `return`/`raise` (routed to `self.cfg.exit`), it's
        // a non-local jump, not a normal fallthrough. Treating it as a regular
        // branch end here added a spurious direct edge from the loop's
        // header/join into this try's own join node, inflating cyclomatic
        // complexity by one per such branch with no corresponding real control
        // flow (hotspots#231).
        let loop_escape_targets: Vec<NodeId> = self
            .loop_stack
            .last()
            .map(|ctx| vec![ctx.break_target, ctx.continue_target])
            .unwrap_or_default();
        let non_exit: Vec<_> = branch_ends
            .into_iter()
            .filter(|end| *end != self.cfg.exit && !loop_escape_targets.contains(end))
            .collect();
        if !non_exit.is_empty() {
            let join_node = self.cfg.add_node(NodeKind::Join);
            for end in non_exit {
                self.cfg.add_edge(end, join_node);
            }
            self.current_node = Some(join_node);
        } else {
            self.current_node = Some(self.cfg.exit);
        }
    }

    /// Dispatch a tree-sitter node to the task (or inline handler) for its kind
    fn dispatch_node<'a>(&mut self, node: Node<'a>, source: &str, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "if_statement" => stack.push(Task::If { node }),
            "while_statement" => stack.push(Task::Loop { node }),
            "for_statement" | "async_for_statement" => stack.push(Task::Loop { node }),
            "try_statement" => stack.push(Task::Try { node }),
            "with_statement" | "async_with_statement" => {
                // With statement - context manager (doesn't add to CC, just ND)
                // Treat as simple statement followed by block
                self.visit_simple_statement();

                if let Some(body) = find_child_by_kind(node, "block") {
                    stack.push(Task::Block { block: body });
                }
            }
            "match_statement" => stack.push(Task::Match { node }),
            "return_statement" => self.visit_return(),
            "raise_statement" => self.visit_raise(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "expression_statement" => self.visit_expression_statement(&node, source),
            "block" => stack.push(Task::Block { block: node }),
            // Assertions (don't add to CC), assignments, and regular statements
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

fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.is_named())
        .collect()
}

fn children_of_kinds<'a>(node: &Node<'a>, kinds: &[&str]) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| kinds.contains(&c.kind()))
        .collect()
}

/// Check if a `case_clause` is an unconditional wildcard (`case _:`) with no
/// guard clause and a single pattern, making it always match once reached.
fn is_wildcard_case(case: &Node, source: &str) -> bool {
    // A guard (`case _ if ...:`) makes the match conditional, not exhaustive.
    if find_child_by_kind(*case, "if_clause").is_some() {
        return false;
    }

    let mut cursor = case.walk();
    let patterns: Vec<_> = case
        .children(&mut cursor)
        .filter(|child| child.kind() == "case_pattern")
        .collect();

    // Multiple comma-separated patterns (`case 1, 2:`) aren't a bare wildcard.
    let [pattern] = patterns.as_slice() else {
        return false;
    };

    pattern.named_child_count() == 0
        && pattern
            .utf8_text(source.as_bytes())
            .map(|text| text == "_")
            .unwrap_or(false)
}

/// Check if expression contains control flow (comprehensions with if, ternary, boolean operators)
///
/// Walks the expression with an explicit stack so deeply nested expressions
/// cannot overflow the call stack.
fn has_control_flow_in_expression(node: &Node, _source: &str) -> bool {
    let mut stack = vec![*node];
    while let Some(node) = stack.pop() {
        match node.kind() {
            // Comprehensions with if clause add to CC
            "list_comprehension"
            | "dictionary_comprehension"
            | "set_comprehension"
            | "generator_expression" => {
                let mut cursor = node.walk();
                if node
                    .children(&mut cursor)
                    .any(|child| child.kind() == "if_clause")
                {
                    return true;
                }
            }
            // Ternary expression (conditional_expression) adds to CC
            "conditional_expression" => return true,
            // Boolean operators (and, or) add to CC
            "boolean_operator" => return true,
            _ => {
                let mut cursor = node.walk();
                stack.extend(node.children(&mut cursor));
            }
        }
    }
    false
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
        let func_node = root
            .children(&mut cursor)
            .find(|n| n.kind() == "function_definition" || n.kind() == "async_function_definition")
            .expect("No function found in test source");

        let start_byte = func_node.start_byte();

        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test_func".to_string()),
            span: SourceSpan::new(
                start_byte,
                func_node.end_byte(),
                func_node.start_position().row as u32 + 1,
                func_node.end_position().row as u32 + 1,
                func_node.start_position().column as u32,
            ),
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

    /// McCabe cyclomatic complexity = E - N + 2, consistent with the fix in
    /// `metrics.rs`'s `calculate_cc_from_cfg` (both N and E now count the
    /// full graph including entry/exit).
    fn mccabe_cc(cfg: &Cfg) -> i64 {
        cfg.edge_count() as i64 - cfg.node_count() as i64 + 2
    }

    #[test]
    fn test_if_no_else_does_not_inflate_cc_hotspots_230() {
        // Regression for #230: `build_from_block` used to wire every nested
        // block's tail directly to the function exit, adding a spurious extra
        // edge even for a branch with no `else` at all (not touched by #228's
        // fix, which only addressed the `has_else` fallthrough edge). One
        // decision point (the `if`) should give cc = 1 + 1 = 2, not 3.
        let source = r#"
def test_func(x):
    if x > 0:
        y = 1
    return y
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);
        assert_eq!(mccabe_cc(&cfg), 2);
    }

    #[test]
    fn test_if_else_does_not_inflate_cc_hotspots_230() {
        // Regression for #230: a two-way if/else is one decision point, so
        // cc = 1 + 1 = 2 — not 4, which is what two spurious direct-to-exit
        // edges (one per branch) would produce.
        let source = r#"
def test_func(x):
    if x > 0:
        y = 1
    else:
        y = 2
    return y
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);
        assert_eq!(mccabe_cc(&cfg), 2);
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

    #[test]
    fn test_match_statement_branches_per_case() {
        // Regression for #139: each `case` clause should be its own CFG
        // branch, not a single collapsed conditional node.
        let source = r#"
def test_func(value):
    match value:
        case 0:
            return "zero"
        case 1:
            return "one"
        case _:
            return "other"
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        assert!(cfg.validate().is_ok(), "match CFG should be valid");

        // Two non-wildcard cases each contribute a Condition node, plus a
        // Statement node per case body and a shared Join node.
        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert_eq!(
            condition_count, 2,
            "expected one condition node per non-wildcard case"
        );

        let join_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Join))
            .count();
        assert_eq!(join_count, 1, "expected a single shared join node");
    }

    #[test]
    fn test_match_statement_wildcard_catch_all() {
        // A wildcard `case _:` with no guard should not add its own
        // condition node - it always matches once reached, like `else`.
        let source = r#"
def test_func(value):
    match value:
        case 1:
            x = 1
        case _:
            x = 0
    return x
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        assert!(cfg.validate().is_ok(), "match CFG should be valid");

        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert_eq!(
            condition_count, 1,
            "wildcard case should not add its own condition node"
        );
    }

    #[test]
    fn test_match_statement_non_exhaustive() {
        // Without a wildcard case, the CFG must still be valid and reachable
        // even though none of the cases may match at runtime.
        let source = r#"
def test_func(value):
    match value:
        case 1:
            return "one"
        case 2:
            return "two"
    return "default"
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        assert!(
            cfg.validate().is_ok(),
            "non-exhaustive match CFG should still be valid"
        );

        let condition_count = cfg
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Condition))
            .count();
        assert_eq!(
            condition_count, 2,
            "expected one condition node per case when there is no wildcard"
        );
    }

    #[test]
    fn test_try_except_finally_all_return() {
        // Regression: when both try and except terminate with return,
        // the finally block must still be reachable (not produce "Nodes not
        // reachable from entry" validation errors).
        let source = r#"
def request(self, method, timeout=60.0):
    rid = self._next_id()
    try:
        return self.q.get(timeout=timeout)
    except Exception:
        return {"error": "timeout"}
    finally:
        self.cleanup(rid)
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);

        // validate() would panic with "Nodes not reachable from entry" before the fix
        assert!(
            cfg.validate().is_ok(),
            "CFG should be valid when try+except both return and finally is present"
        );
    }

    #[test]
    fn test_try_except_continue_inside_for_does_not_inflate_cc_hotspots_231() {
        // Regression for #231: an except clause ending in `continue` had
        // already been routed to the loop header by `visit_continue`, but
        // `visit_try`'s join-wiring treated that redirected node as a normal
        // branch end anyway, adding a spurious loop_header -> try_join edge.
        // One `for` + one `except` = 2 decision points, so cc = 1 + 2 = 3.
        let source = r#"
def test_func(items):
    total = 0
    for item in items:
        try:
            total += item
        except ValueError:
            continue
    return total
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);
        assert_eq!(mccabe_cc(&cfg), 3);
    }

    #[test]
    fn test_try_except_break_inside_for_does_not_inflate_cc_hotspots_231() {
        // Same bug, via `break` instead of `continue` (routes to the loop's
        // break_target/join rather than continue_target/header).
        let source = r#"
def test_func(items):
    total = 0
    for item in items:
        try:
            total += item
        except ValueError:
            break
    return total
"#;
        let function = make_python_function(source);
        let builder = PythonCfgBuilder;
        let cfg = builder.build(&function);
        assert_eq!(mccabe_cc(&cfg), 3);
    }

    /// Deep nesting must build a full CFG through the real parse+discover+build pipeline.
    #[test]
    fn test_deeply_nested_if_else_does_not_overflow_and_builds_real_cfg() {
        // tree-sitter-python stops yielding a function somewhere between 200 and 300
        // nested indent levels, so this is as deep as the parser itself allows.
        const DEPTH: usize = 200;

        let mut source = String::from("def test(x):\n");
        for i in 0..DEPTH {
            source.push_str(&" ".repeat(i + 1));
            source.push_str("if x > 0:\n");
        }
        source.push_str(&" ".repeat(DEPTH + 1));
        source.push_str("x = 1\n");

        let node_count = std::thread::Builder::new()
            .stack_size(6 * 1024 * 1024)
            .spawn(move || {
                use crate::language::{LanguageParser, PythonParser};
                let parser = PythonParser::new().unwrap();
                let module = parser.parse(&source, "test.py").unwrap();
                let functions = module.discover_functions(0, &source);
                assert_eq!(functions.len(), 1);
                PythonCfgBuilder.build(&functions[0]).node_count()
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
