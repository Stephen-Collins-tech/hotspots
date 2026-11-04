//! PHP CFG builder implementation
//!
//! Walks the tree-sitter-php AST iteratively via an explicit heap-allocated
//! work stack (`Task`, driven by the `run` loop, with results threaded through
//! `self.current_node`) instead of native recursion, so nesting depth is
//! bounded by memory, not the thread stack (same design as the Go and Rust
//! builders).
//!
//! `current_node == None` means the current path has terminated (`return`,
//! `throw`, `break`, ...); statements visited in that state are dead code and
//! are skipped. Join nodes are created lazily, only when a live path reaches
//! them.

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use crate::language::php::parser::FUNCTION_KINDS;
use crate::language::tree_sitter_utils::{find_function_by_start, with_cached_php_tree};
use tree_sitter::Node;

/// PHP CFG builder
///
/// Builds control flow graphs from PHP function bodies parsed with tree-sitter.
pub struct PhpCfgBuilder;

impl CfgBuilder for PhpCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_php();

        let result = with_cached_php_tree(source, |root| {
            let func_node = find_function_by_start(root, function.span.start, FUNCTION_KINDS)?;
            let body_node = func_node.child_by_field_name("body")?;
            let mut builder = PhpCfgBuilderState::new();
            builder.build_from_body(body_node, source);
            Some(builder.cfg)
        });

        result.unwrap_or_else(|| {
            let mut cfg = Cfg::new();
            cfg.add_edge(cfg.entry, cfg.exit);
            cfg
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextKind {
    /// `while`, `for`, `foreach`: `continue` jumps to the header
    Loop,
    /// `do { } while ()`: `continue` jumps to the condition, created lazily
    DoLoop,
    /// `switch`: `continue` behaves like `break`
    Switch,
}

struct LoopContext {
    kind: ContextKind,
    /// Lazily created: `None` until a `break`, fall-off-the-end or fallback
    /// edge actually needs it.
    break_target: Option<NodeId>,
    /// Loop header (`Loop`), lazily created condition node (`DoLoop`), `None` for switch
    continue_target: Option<NodeId>,
}

/// Pending unit of work on the explicit stack. The "After*" variants run what
/// used to happen after a recursive call returned.
enum Task<'a> {
    /// Sequential list of statements.
    Children {
        children: Vec<Node<'a>>,
        idx: usize,
    },
    VisitNode {
        node: Node<'a>,
    },
    If {
        node: Node<'a>,
    },
    /// Enter branch `idx` of an if/elseif/else chain. `entry` is the node the
    /// next branch hangs off: the previous branch's condition.
    IfNext {
        /// `(has_condition, body)`; only the last branch may lack a condition (`else`)
        branches: Vec<(bool, Option<Node<'a>>)>,
        idx: usize,
        entry: NodeId,
        ends: Vec<NodeId>,
    },
    IfAfterBody {
        branches: Vec<(bool, Option<Node<'a>>)>,
        idx: usize,
        next_entry: NodeId,
        ends: Vec<NodeId>,
    },
    /// `while`, `for`, `foreach`
    Loop {
        node: Node<'a>,
    },
    AfterLoopBody {
        loop_header: NodeId,
    },
    DoWhile {
        node: Node<'a>,
    },
    AfterDoBody {
        body_start: NodeId,
    },
    Switch {
        node: Node<'a>,
    },
    CaseList {
        idx: usize,
        condition_node: NodeId,
        cases: Vec<Node<'a>>,
        pos: usize,
    },
    CaseBody {
        case: Node<'a>,
        condition_node: NodeId,
        /// End of the previous case when it falls through into this one
        prev_end: Option<NodeId>,
    },
    AfterSwitchCases {
        idx: usize,
        condition_node: NodeId,
        has_default: bool,
    },
    Try {
        node: Node<'a>,
    },
    TryNext {
        try_entry: NodeId,
        catches: Vec<Node<'a>>,
        idx: usize,
        ends: Vec<NodeId>,
        finally: Option<Node<'a>>,
        /// Whether the branch that just ran should be recorded as a live end
        collect: bool,
    },
}

struct PhpCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    loop_stack: Vec<LoopContext>,
}

impl PhpCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;
        PhpCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
        }
    }

    fn get_or_create_break_target(&mut self, idx: usize) -> NodeId {
        if let Some(id) = self.loop_stack[idx].break_target {
            return id;
        }
        let id = self.cfg.add_node(NodeKind::Join);
        self.loop_stack[idx].break_target = Some(id);
        id
    }

    fn get_or_create_continue_target(&mut self, idx: usize) -> NodeId {
        if let Some(id) = self.loop_stack[idx].continue_target {
            return id;
        }
        let id = self.cfg.add_node(NodeKind::LoopHeader);
        self.loop_stack[idx].continue_target = Some(id);
        id
    }

    fn add_statement_after(&mut self, from: NodeId) -> NodeId {
        let node = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from, node);
        node
    }

    /// Build the CFG for a function body (a `compound_statement`, or the
    /// expression of an arrow function).
    fn build_from_body(&mut self, body: Node<'_>, source: &str) {
        let mut stack = vec![Task::VisitNode { node: body }];
        self.run(&mut stack, source);

        // Fall off the end of the function
        if let Some(last) = self.current_node {
            let has_exit_edge = self
                .cfg
                .edges
                .iter()
                .any(|e| e.from == last && e.to == self.cfg.exit);
            if !has_exit_edge {
                self.cfg.add_edge(last, self.cfg.exit);
            }
        }
    }

    fn run(&mut self, stack: &mut Vec<Task<'_>>, source: &str) {
        while let Some(task) = stack.pop() {
            match task {
                Task::Children { children, idx } => {
                    if idx < children.len() {
                        let node = children[idx];
                        stack.push(Task::Children {
                            children,
                            idx: idx + 1,
                        });
                        stack.push(Task::VisitNode { node });
                    }
                }
                Task::VisitNode { node } => self.dispatch_node(node, source, stack),
                Task::If { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let mut branches = vec![(true, node.child_by_field_name("body"))];
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        match child.kind() {
                            "else_if_clause" => {
                                branches.push((true, child.child_by_field_name("body")))
                            }
                            "else_clause" => {
                                branches.push((false, child.child_by_field_name("body")))
                            }
                            _ => {}
                        }
                    }
                    stack.push(Task::IfNext {
                        branches,
                        idx: 0,
                        entry: from_node,
                        ends: Vec::new(),
                    });
                }
                Task::IfNext {
                    branches,
                    idx,
                    entry,
                    ends,
                } => {
                    if idx == branches.len() {
                        let has_else = !branches[idx - 1].0;
                        if !ends.is_empty() || !has_else {
                            let join = self.cfg.add_node(NodeKind::Join);
                            for end in ends {
                                self.cfg.add_edge(end, join);
                            }
                            if !has_else {
                                // Last condition can be false with nothing to run
                                self.cfg.add_edge(entry, join);
                            }
                            self.current_node = Some(join);
                        } else {
                            self.current_node = None;
                        }
                        continue;
                    }

                    let (has_condition, body) = branches[idx];
                    let (start, next_entry) = if has_condition {
                        let condition = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(entry, condition);
                        (self.add_statement_after(condition), condition)
                    } else {
                        (self.add_statement_after(entry), entry)
                    };
                    self.current_node = Some(start);
                    stack.push(Task::IfAfterBody {
                        branches,
                        idx,
                        next_entry,
                        ends,
                    });
                    if let Some(body) = body {
                        stack.push(Task::VisitNode { node: body });
                    }
                }
                Task::IfAfterBody {
                    branches,
                    idx,
                    next_entry,
                    mut ends,
                } => {
                    if let Some(end) = self.current_node {
                        ends.push(end);
                    }
                    stack.push(Task::IfNext {
                        branches,
                        idx: idx + 1,
                        entry: next_entry,
                        ends,
                    });
                }
                Task::Loop { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, loop_header);
                    let body_start = self.add_statement_after(loop_header);

                    self.loop_stack.push(LoopContext {
                        kind: ContextKind::Loop,
                        break_target: None,
                        continue_target: Some(loop_header),
                    });

                    self.current_node = Some(body_start);
                    stack.push(Task::AfterLoopBody { loop_header });
                    if let Some(body) = node.child_by_field_name("body") {
                        stack.push(Task::VisitNode { node: body });
                    }
                }
                Task::AfterLoopBody { loop_header } => {
                    // Back edge to the header
                    if let Some(body_end) = self.current_node {
                        self.cfg.add_edge(body_end, loop_header);
                    }
                    // The loop condition can always be false: header -> join
                    let idx = self.loop_stack.len() - 1;
                    let join = self.get_or_create_break_target(idx);
                    self.cfg.add_edge(loop_header, join);
                    self.loop_stack.pop();
                    self.current_node = Some(join);
                }
                Task::DoWhile { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let body_start = self.add_statement_after(from_node);
                    self.loop_stack.push(LoopContext {
                        kind: ContextKind::DoLoop,
                        break_target: None,
                        continue_target: None,
                    });
                    self.current_node = Some(body_start);
                    stack.push(Task::AfterDoBody { body_start });
                    if let Some(body) = node.child_by_field_name("body") {
                        stack.push(Task::VisitNode { node: body });
                    }
                }
                Task::AfterDoBody { body_start } => {
                    let idx = self.loop_stack.len() - 1;
                    let body_end = self.current_node;
                    let has_continue = self.loop_stack[idx].continue_target.is_some();
                    if body_end.is_some() || has_continue {
                        // The condition is reachable (fell off the body or via `continue`)
                        let condition = self.get_or_create_continue_target(idx);
                        if let Some(end) = body_end {
                            self.cfg.add_edge(end, condition);
                        }
                        self.cfg.add_edge(condition, body_start);
                        let join = self.get_or_create_break_target(idx);
                        self.cfg.add_edge(condition, join);
                        self.current_node = Some(join);
                    } else {
                        // Body always terminates: only a `break` can leave the loop
                        self.current_node = self.loop_stack[idx].break_target;
                    }
                    self.loop_stack.pop();
                }
                Task::Switch { node } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    let idx = self.loop_stack.len();
                    self.loop_stack.push(LoopContext {
                        kind: ContextKind::Switch,
                        break_target: None,
                        continue_target: None,
                    });

                    let cases: Vec<Node<'_>> = node
                        .child_by_field_name("body")
                        .map(|block| {
                            let mut cursor = block.walk();
                            block
                                .children(&mut cursor)
                                .filter(|c| {
                                    matches!(c.kind(), "case_statement" | "default_statement")
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let has_default = cases.iter().any(|c| c.kind() == "default_statement");

                    stack.push(Task::AfterSwitchCases {
                        idx,
                        condition_node,
                        has_default,
                    });
                    stack.push(Task::CaseList {
                        idx,
                        condition_node,
                        cases,
                        pos: 0,
                    });
                }
                Task::CaseList {
                    idx,
                    condition_node,
                    cases,
                    pos,
                } => {
                    if pos < cases.len() {
                        let case = cases[pos];
                        // PHP cases fall through: the previous case's end
                        // (if it didn't break/return) flows into this one.
                        let prev_end = if pos > 0 { self.current_node } else { None };
                        stack.push(Task::CaseList {
                            idx,
                            condition_node,
                            cases,
                            pos: pos + 1,
                        });
                        stack.push(Task::CaseBody {
                            case,
                            condition_node,
                            prev_end,
                        });
                    }
                }
                Task::CaseBody {
                    case,
                    condition_node,
                    prev_end,
                } => {
                    let case_start = self.add_statement_after(condition_node);
                    if let Some(prev) = prev_end {
                        self.cfg.add_edge(prev, case_start);
                    }
                    self.current_node = Some(case_start);

                    // Skip the `case <value>:` expression; the rest are statements
                    let value_id = case.child_by_field_name("value").map(|v| v.id());
                    stack.push(Task::Children {
                        children: named_children(&case)
                            .into_iter()
                            .filter(|c| Some(c.id()) != value_id)
                            .collect(),
                        idx: 0,
                    });
                }
                Task::AfterSwitchCases {
                    idx,
                    condition_node,
                    has_default,
                } => {
                    // The last case flows out of the switch
                    if let Some(end) = self.current_node {
                        let join = self.get_or_create_break_target(idx);
                        self.cfg.add_edge(end, join);
                    }
                    // Without `default`, no case may match: condition -> join
                    if !has_default {
                        let join = self.get_or_create_break_target(idx);
                        self.cfg.add_edge(condition_node, join);
                    }
                    let ctx = self.loop_stack.pop().expect("pushed switch context above");
                    self.current_node = ctx.break_target;
                }
                Task::Try { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };
                    let try_entry = self.add_statement_after(current);

                    let mut catches = Vec::new();
                    let mut finally = None;
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        match child.kind() {
                            "catch_clause" => catches.push(child),
                            "finally_clause" => finally = child.child_by_field_name("body"),
                            _ => {}
                        }
                    }

                    self.current_node = Some(try_entry);
                    stack.push(Task::TryNext {
                        try_entry,
                        catches,
                        idx: 0,
                        ends: Vec::new(),
                        finally,
                        collect: true,
                    });
                    if let Some(body) = node.child_by_field_name("body") {
                        stack.push(Task::VisitNode { node: body });
                    }
                }
                Task::TryNext {
                    try_entry,
                    catches,
                    idx,
                    mut ends,
                    finally,
                    collect,
                } => {
                    if collect {
                        if let Some(last) = self.current_node {
                            ends.push(last);
                        }
                    }

                    if idx < catches.len() {
                        // Each catch is a separate branch from the try entry
                        let catch_node = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(try_entry, catch_node);
                        self.current_node = Some(catch_node);
                        stack.push(Task::TryNext {
                            try_entry,
                            catches: catches.clone(),
                            idx: idx + 1,
                            ends,
                            finally,
                            collect: true,
                        });
                        if let Some(body) = catches[idx].child_by_field_name("body") {
                            stack.push(Task::VisitNode { node: body });
                        }
                    } else if ends.is_empty() {
                        // Every branch terminated; code after the try (and the
                        // finally block, on these paths) is unreachable.
                        self.current_node = None;
                    } else {
                        let join = self.cfg.add_node(NodeKind::Statement);
                        for end in ends {
                            self.cfg.add_edge(end, join);
                        }
                        self.current_node = Some(join);
                        // `finally` is modeled only on the paths that complete
                        // normally; paths that return/throw skip it.
                        if let Some(finally) = finally {
                            stack.push(Task::VisitNode { node: finally });
                        }
                    }
                }
            }
        }
    }

    /// Dispatch a single node: compound/control nodes push further `Task`s;
    /// simple nodes are handled inline since they never recurse.
    fn dispatch_node<'a>(&mut self, node: Node<'a>, source: &str, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "if_statement" => stack.push(Task::If { node }),
            "while_statement" | "for_statement" | "foreach_statement" => {
                stack.push(Task::Loop { node })
            }
            "do_statement" => stack.push(Task::DoWhile { node }),
            "switch_statement" => stack.push(Task::Switch { node }),
            "try_statement" => stack.push(Task::Try { node }),
            "return_statement" | "exit_statement" => self.visit_terminator(),
            "break_statement" => self.visit_break(&node, source),
            "continue_statement" => self.visit_continue(&node, source),
            "goto_statement" => self.visit_goto(),
            "named_label_statement" => self.visit_label(),
            "expression_statement" => {
                if is_throw(&node) {
                    self.visit_terminator();
                } else {
                    self.visit_simple_statement();
                }
            }
            "compound_statement" | "colon_block" => stack.push(Task::Children {
                children: named_children(&node),
                idx: 0,
            }),
            // `declare(ticks=1) { ... }` wraps statements; `declare(strict_types=1);` has none
            "declare_statement" => stack.push(Task::Children {
                children: named_children(&node)
                    .into_iter()
                    .filter(|c| c.kind() != "declare_directive")
                    .collect(),
                idx: 0,
            }),
            // Inline HTML between `?>` and `<?php`, and comments, carry no control flow
            "text_interpolation" | "text" | "comment" => {}
            _ => self.visit_simple_statement(),
        }
    }

    fn visit_simple_statement(&mut self) {
        if let Some(from_node) = self.current_node {
            self.current_node = Some(self.add_statement_after(from_node));
        }
    }

    /// `return`, `throw`, `exit`/`die`: the path ends at the function exit.
    fn visit_terminator(&mut self) {
        if let Some(from_node) = self.current_node {
            let node = self.add_statement_after(from_node);
            self.cfg.add_edge(node, self.cfg.exit);
            self.current_node = None;
        }
    }

    /// `break N`: leave the N-th enclosing loop or switch.
    fn visit_break(&mut self, node: &Node, source: &str) {
        if let Some(from_node) = self.current_node {
            let Some(idx) = self.target_index(jump_level(node, source)) else {
                return;
            };
            let target = self.get_or_create_break_target(idx);
            let break_node = self.add_statement_after(from_node);
            self.cfg.add_edge(break_node, target);
            self.current_node = None;
        }
    }

    /// `continue N`: next iteration of the N-th enclosing loop. A `switch`
    /// counts as a loop that runs once, so `continue` targeting it is `break`.
    fn visit_continue(&mut self, node: &Node, source: &str) {
        if let Some(from_node) = self.current_node {
            let Some(idx) = self.target_index(jump_level(node, source)) else {
                return;
            };
            let target = match self.loop_stack[idx].kind {
                ContextKind::Loop => self.loop_stack[idx]
                    .continue_target
                    .expect("loop header set at creation"),
                ContextKind::DoLoop => self.get_or_create_continue_target(idx),
                ContextKind::Switch => self.get_or_create_break_target(idx),
            };
            let continue_node = self.add_statement_after(from_node);
            self.cfg.add_edge(continue_node, target);
            self.current_node = None;
        }
    }

    /// Index into `loop_stack` for a `break`/`continue` of the given level;
    /// `None` outside any loop/switch. Levels past the outermost clamp to it.
    fn target_index(&self, level: usize) -> Option<usize> {
        if self.loop_stack.is_empty() {
            None
        } else {
            Some(self.loop_stack.len().saturating_sub(level.max(1)))
        }
    }

    /// `goto` is a branch (+1 CC) after which the path leaves; jump targets
    /// aren't tracked, so it is modeled as an edge straight to the exit
    /// (same approximation as the C and Go builders).
    fn visit_goto(&mut self) {
        if let Some(from_node) = self.current_node {
            let goto_node = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(from_node, goto_node);
            self.cfg.add_edge(goto_node, self.cfg.exit);
            self.current_node = None;
        }
    }

    /// A label is a `goto` target, so it always gets a node, even when the
    /// preceding code terminated.
    fn visit_label(&mut self) {
        let label_node = self.cfg.add_node(NodeKind::Statement);
        let from = self.current_node.unwrap_or(self.cfg.entry);
        self.cfg.add_edge(from, label_node);
        self.current_node = Some(label_node);
    }
}

/// Collect every named child of `node`, in order, dropping comments.
fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.is_named() && c.kind() != "comment")
        .collect()
}

/// The `N` of `break N` / `continue N` (default 1).
fn jump_level(node: &Node, source: &str) -> usize {
    named_children(node)
        .first()
        .and_then(|n| source[n.start_byte()..n.end_byte()].trim().parse().ok())
        .unwrap_or(1)
}

/// True for an expression statement that is a `throw` expression.
fn is_throw(node: &Node) -> bool {
    named_children(node)
        .first()
        .is_some_and(|c| c.kind() == "throw_expression")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::parser::LanguageParser;
    use crate::language::PhpParser;

    /// Build the CFG of the first function in `source`.
    fn cfg_of(source: &str) -> Cfg {
        let parser = PhpParser::new().unwrap();
        let module = parser.parse(source, "test.php").unwrap();
        let functions = module.discover_functions(0, source);
        assert!(!functions.is_empty(), "no function found");
        let cfg = PhpCfgBuilder.build(&functions[0]);
        cfg.validate().expect("CFG should validate");
        cfg
    }

    /// Cyclomatic complexity from the CFG: E - N + 2
    fn cc(cfg: &Cfg) -> usize {
        cfg.edge_count() + 2 - cfg.node_count()
    }

    #[test]
    fn test_empty_function() {
        let cfg = cfg_of("<?php function f() {}");
        assert_eq!(cc(&cfg), 1);
    }

    #[test]
    fn test_straight_line() {
        let cfg = cfg_of("<?php function f() { $a = 1; $b = 2; return $a + $b; }");
        assert_eq!(cc(&cfg), 1);
    }

    #[test]
    fn test_if_else_chain() {
        let cfg = cfg_of(
            "<?php function f($a) { if ($a > 1) { x(); } elseif ($a) { y(); } else { z(); } }",
        );
        assert_eq!(cc(&cfg), 3);
    }

    #[test]
    fn test_else_if_with_space_and_alt_syntax() {
        let spaced =
            cfg_of("<?php function f($a) { if ($a) { x(); } else if ($b) { y(); } else { z(); } }");
        assert_eq!(cc(&spaced), 3);
        let alt =
            cfg_of("<?php function f($a) { if ($a): x(); elseif ($b): y(); else: z(); endif; }");
        assert_eq!(cc(&alt), 3);
    }

    #[test]
    fn test_if_without_braces() {
        let cfg = cfg_of("<?php function f($a) { if ($a) x(); else y(); }");
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_if_all_branches_return() {
        let cfg = cfg_of("<?php function f($a) { if ($a) { return 1; } else { return 2; } }");
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_loops() {
        for body in [
            "while ($a) { x(); }",
            "for ($i = 0; $i < 3; $i++) { x(); }",
            "foreach ($a as $v) { x(); }",
            "do { x(); } while ($a);",
            "while ($a): x(); endwhile;",
            "foreach ($a as $v): x(); endforeach;",
        ] {
            let cfg = cfg_of(&format!("<?php function f($a) {{ {body} }}"));
            assert_eq!(cc(&cfg), 2, "loop: {body}");
        }
    }

    #[test]
    fn test_break_and_continue() {
        let cfg = cfg_of(
            "<?php function f($a) { foreach ($a as $v) { if ($v) { continue; } if ($v > 1) { break; } x(); } }",
        );
        assert_eq!(cc(&cfg), 4);
    }

    #[test]
    fn test_nested_break_levels() {
        let cfg = cfg_of(
            "<?php function f($a) { foreach ($a as $v) { foreach ($v as $w) { if ($w) { break 2; } } } }",
        );
        assert_eq!(cc(&cfg), 4);
        let cfg = cfg_of(
            "<?php function f($a) { foreach ($a as $v) { foreach ($v as $w) { if ($w) { continue 2; } } } }",
        );
        assert_eq!(cc(&cfg), 4);
    }

    #[test]
    fn test_do_while_always_returns_with_continue() {
        // `continue` makes the do-while condition reachable
        let cfg = cfg_of("<?php function f($a) { do { continue; } while ($a); }");
        assert_eq!(cc(&cfg), 2);
        // Body always returns, no break: the loop never completes
        let cfg = cfg_of("<?php function f($a) { do { return 1; } while ($a); }");
        assert_eq!(cc(&cfg), 1);
    }

    #[test]
    fn test_switch_with_default_and_fallthrough() {
        let cfg = cfg_of(
            "<?php function f($a) { switch ($a) { case 1: x(); case 2: y(); break; default: z(); } }",
        );
        // case 1, case 2 (default adds an edge, not a decision beyond the cases)
        assert_eq!(cc(&cfg), 3);
    }

    #[test]
    fn test_switch_without_default() {
        let cfg = cfg_of(
            "<?php function f($a) { switch ($a) { case 1: x(); break; case 2: y(); break; } }",
        );
        assert_eq!(cc(&cfg), 3);
    }

    #[test]
    fn test_switch_all_cases_return() {
        let cfg = cfg_of(
            "<?php function f($a) { switch ($a) { case 1: return 1; default: return 2; } return 3; }",
        );
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_switch_alt_syntax() {
        let cfg = cfg_of(
            "<?php function f($a) { switch ($a): case 1: x(); break; default: y(); endswitch; }",
        );
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_continue_in_switch_acts_like_break() {
        let cfg = cfg_of(
            "<?php function f($a) { foreach ($a as $v) { switch ($v) { case 1: continue; default: x(); } } }",
        );
        cfg.validate().unwrap();
    }

    #[test]
    fn test_try_catch_finally() {
        let cfg = cfg_of(
            "<?php function f() { try { x(); } catch (A | B $e) { y(); } catch (C $e) { z(); } finally { w(); } }",
        );
        assert_eq!(cc(&cfg), 3);
    }

    #[test]
    fn test_try_all_paths_terminate() {
        let cfg = cfg_of(
            "<?php function f() { try { return x(); } catch (A $e) { throw $e; } finally { w(); } }",
        );
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_throw_and_exit() {
        let cfg = cfg_of("<?php function f($a) { if ($a) { throw new E(); } exit(1); }");
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_goto_and_label() {
        let cfg = cfg_of("<?php function f($a) { if ($a) { goto end; } x(); end: y(); }");
        cfg.validate().unwrap();
        // The goto path is an early exit, like `return` (+1 for the `if` itself)
        assert_eq!(cc(&cfg), 2);
    }

    #[test]
    fn test_dead_code_after_return() {
        let cfg = cfg_of("<?php function f() { return 1; if ($a) { x(); } while ($b) { y(); } }");
        assert_eq!(cc(&cfg), 1);
    }

    #[test]
    fn test_arrow_function_and_closure() {
        let parser = PhpParser::new().unwrap();
        let source = "<?php function f() { $g = fn($x) => $x + 1; $h = function () { if ($a) { return 1; } return 2; }; }";
        let module = parser.parse(source, "test.php").unwrap();
        let functions = module.discover_functions(0, source);
        assert_eq!(functions.len(), 3);
        for (function, expected) in functions.iter().zip([1, 1, 2]) {
            let cfg = PhpCfgBuilder.build(function);
            cfg.validate().unwrap();
            assert_eq!(cc(&cfg), expected);
        }
    }

    #[test]
    fn test_comments_do_not_add_nodes() {
        let with = cfg_of("<?php function f() { // c\n /* d */ $a = 1; # e\n }");
        let without = cfg_of("<?php function f() { $a = 1; }");
        assert_eq!(with.node_count(), without.node_count());
    }

    #[test]
    fn test_deeply_nested_does_not_overflow() {
        let depth = 3000;
        let mut source = String::from("<?php function f($a) {\n");
        for _ in 0..depth {
            source.push_str("if ($a) {\n");
        }
        source.push_str("x();\n");
        for _ in 0..depth {
            source.push_str("}\n");
        }
        source.push('}');

        let handle = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let cfg = cfg_of(&source);
                assert_eq!(cc(&cfg), depth + 1);
            })
            .unwrap();
        handle
            .join()
            .expect("deep nesting must not overflow the stack");
    }
}
