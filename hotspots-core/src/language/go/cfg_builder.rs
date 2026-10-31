//! Go CFG builder implementation
//!
//! Walks the tree-sitter-go AST iteratively via an explicit heap-allocated
//! work stack (`Task`, driven by the `run` loop, with results threaded
//! through `self.current_node`) instead of native function-call recursion
//! between `visit_*`/`build_from_block` methods. This removes the depth
//! bound entirely — nesting depth is now limited by available heap/memory,
//! not by the calling thread's stack — closing the structural gap tracked in
//! https://github.com/Stephen-Collins-tech/hotspots/issues/160 for the Go
//! builder (see PR #245/#263 for the equivalent Rust/JS-TS conversions this
//! follows).

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

/// Pending unit of work on the explicit stack.
///
/// The "visit"-shaped variants (`If`, `For`, `Switch`, ...) mirror what the
/// old recursive `visit_*`/`build_from_block` methods did on the way down the
/// call stack; the "After*" variants mirror what they did with the recursive
/// result on the way back up, now driven by a loop instead of native stack
/// unwinding. Each variant borrows from the same tree-sitter tree, which
/// outlives the whole build (owned by `with_cached_go_tree`'s closure). The
/// "result" of whatever nested work just ran is read from
/// `self.current_node` (exactly as the recursive methods used it before), so
/// no separate result-passing field is needed.
enum Task<'a> {
    /// Sequential list of named children (a block body, a case body, the
    /// `statement_list` wrapper, ...).
    Children {
        children: Vec<Node<'a>>,
        idx: usize,
    },
    VisitNode {
        node: Node<'a>,
    },
    /// `block`/`build_from_block`: visit children, then force an edge to
    /// `exit` from the last node if one doesn't already exist.
    FromBlock {
        block: Node<'a>,
    },
    AfterFromBlock,
    If {
        node: Node<'a>,
    },
    AfterIfThen {
        node: Node<'a>,
        condition_node: NodeId,
        then_start: NodeId,
    },
    AfterIfElse {
        then_end: NodeId,
        join_node: NodeId,
        else_start: NodeId,
    },
    For {
        node: Node<'a>,
    },
    AfterForBody {
        loop_header: NodeId,
    },
    Switch {
        node: Node<'a>,
    },
    /// Shared by switch and select: walk case nodes sequentially, since each
    /// case's CFG wiring depends on `current_node`/`loop_stack` state left
    /// behind by the previous one.
    CaseList {
        idx: usize,
        condition_node: NodeId,
        cases: Vec<Node<'a>>,
        pos: usize,
    },
    CaseBody {
        case: Node<'a>,
        condition_node: NodeId,
        idx: usize,
    },
    AfterCaseBody {
        idx: usize,
    },
    AfterSwitchCases {
        idx: usize,
        condition_node: NodeId,
        has_default: bool,
    },
    Select {
        node: Node<'a>,
    },
    AfterSelectCases,
    Labeled {
        node: Node<'a>,
    },
    AfterLabeled,
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

    /// Build CFG from a block node.
    fn build_from_block(&mut self, block: Node<'_>, source: &str) {
        let mut stack = vec![Task::FromBlock { block }];
        self.run(&mut stack, source);
    }

    /// Drive the explicit work-stack loop. Pushing a `Task` here replaces
    /// what used to be a native recursive call; the "After*" variants
    /// replace what used to run after that recursive call returned.
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
                Task::FromBlock { block } => {
                    stack.push(Task::AfterFromBlock);
                    stack.push(Task::Children {
                        children: named_children(&block),
                        idx: 0,
                    });
                }
                Task::AfterFromBlock => {
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
                Task::If { node } => {
                    let Some(from_node) = self.current_node else {
                        // Unreachable (dead code after return/panic/etc.): nothing to wire up.
                        continue;
                    };

                    // Condition node
                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    // Then branch (consequence block)
                    let then_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, then_start);

                    stack.push(Task::AfterIfThen {
                        node,
                        condition_node,
                        then_start,
                    });
                    if let Some(consequence) = find_child_by_kind(node, "block") {
                        self.current_node = Some(then_start);
                        stack.push(Task::FromBlock { block: consequence });
                    }
                }
                Task::AfterIfThen {
                    node,
                    condition_node,
                    then_start,
                } => {
                    let then_end = self.current_node.unwrap_or(then_start);

                    // Else branch (alternative)
                    let join_node = self.cfg.add_node(NodeKind::Join);

                    if let Some(alternative) = find_child_by_field(node, "alternative") {
                        let else_start = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(condition_node, else_start);
                        self.current_node = Some(else_start);

                        stack.push(Task::AfterIfElse {
                            then_end,
                            join_node,
                            else_start,
                        });
                        stack.push(Task::VisitNode { node: alternative });
                    } else {
                        // No else branch - condition can go directly to join
                        self.cfg.add_edge(condition_node, join_node);
                        if then_end != self.cfg.exit {
                            self.cfg.add_edge(then_end, join_node);
                        }
                        self.current_node = Some(join_node);
                    }
                }
                Task::AfterIfElse {
                    then_end,
                    join_node,
                    else_start,
                } => {
                    let else_end = self.current_node.unwrap_or(else_start);

                    if then_end != self.cfg.exit {
                        self.cfg.add_edge(then_end, join_node);
                    }
                    if else_end != self.cfg.exit {
                        self.cfg.add_edge(else_end, join_node);
                    }
                    self.current_node = Some(join_node);
                }
                Task::For { node } => {
                    let Some(from_node) = self.current_node else {
                        // Unreachable (dead code after return/panic/etc.): nothing to wire up.
                        continue;
                    };

                    // Loop header
                    let loop_header = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, loop_header);

                    // Loop body
                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(loop_header, body_start);

                    // Push loop context for break/continue. The join node is
                    // created lazily below via get_or_create_break_target,
                    // but a `for` loop's header always has an unconditional
                    // "exit" edge to it (the loop condition can always be
                    // false, or there is none), so it will always end up
                    // created.
                    let label = self.pending_label.take();
                    self.loop_stack.push(LoopContext {
                        break_target: None,
                        continue_target: Some(loop_header),
                        label,
                    });

                    stack.push(Task::AfterForBody { loop_header });
                    if let Some(body) = find_child_by_kind(node, "block") {
                        self.current_node = Some(body_start);
                        stack.push(Task::FromBlock { block: body });
                    }
                }
                Task::AfterForBody { loop_header } => {
                    // Back edge to loop header
                    if let Some(body_end) = self.current_node {
                        if body_end != self.cfg.exit {
                            self.cfg.add_edge(body_end, loop_header);
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
                Task::Switch { node } => {
                    let Some(from_node) = self.current_node else {
                        // Unreachable (dead code after return/panic/etc.): nothing to wire up.
                        continue;
                    };

                    // Switch condition
                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    // Push break context (switch/select break independently
                    // of any enclosing loop; `continue` is not valid here,
                    // so continue_target is None and unlabeled continue
                    // skips over this context). The join node itself is
                    // created lazily: if every case terminates control flow
                    // (e.g. `return` in every case) and there's a default
                    // (so no fallback-to-join edge is added either), nothing
                    // ever needs a join node, and creating one eagerly would
                    // leave it unreachable.
                    let idx = self.loop_stack.len();
                    let label = self.pending_label.take();
                    self.loop_stack.push(LoopContext {
                        break_target: None,
                        continue_target: None,
                        label,
                    });

                    // Regular switches use "expression_case"; type switches
                    // (switch x.(type) { case T: ... }) use "type_case".
                    let mut cursor = node.walk();
                    let cases: Vec<Node<'_>> = node
                        .children(&mut cursor)
                        .filter(|c| is_switch_case_kind(c.kind()))
                        .collect();
                    let has_default = cases.iter().any(|c| c.kind() == "default_case");

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
                        stack.push(Task::CaseList {
                            idx,
                            condition_node,
                            cases,
                            pos: pos + 1,
                        });
                        stack.push(Task::CaseBody {
                            case,
                            condition_node,
                            idx,
                        });
                    }
                }
                Task::CaseBody {
                    case,
                    condition_node,
                    idx,
                } => {
                    let case_start = self.cfg.add_node(NodeKind::Statement);
                    // Each case gets an edge from switch/select (for
                    // case matching).
                    self.cfg.add_edge(condition_node, case_start);
                    self.current_node = Some(case_start);

                    // `:` is a punctuation token, already excluded by
                    // `named_children`'s `is_named()` check like every other
                    // unnamed token.
                    stack.push(Task::AfterCaseBody { idx });
                    stack.push(Task::Children {
                        children: named_children(&case),
                        idx: 0,
                    });
                }
                Task::AfterCaseBody { idx } => {
                    if let Some(case_end) = self.current_node {
                        if case_end != self.cfg.exit {
                            let join_node = self.get_or_create_break_target(idx);
                            self.cfg.add_edge(case_end, join_node);
                        }
                    }
                }
                Task::AfterSwitchCases {
                    idx,
                    condition_node,
                    has_default,
                } => {
                    // If there's no default case, no case is guaranteed to
                    // match, so the switch can fall through directly to
                    // join. When a default exists, one case always
                    // executes, so this edge would be spurious.
                    if !has_default {
                        let join_node = self.get_or_create_break_target(idx);
                        self.cfg.add_edge(condition_node, join_node);
                    }

                    let ctx = self.loop_stack.pop().expect("pushed switch context above");
                    self.current_node = ctx.break_target;
                }
                Task::Select { node } => {
                    let Some(from_node) = self.current_node else {
                        // Unreachable (dead code after return/panic/etc.): nothing to wire up.
                        continue;
                    };

                    // Select condition (non-deterministic choice)
                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    // Push break context (select break independently of any
                    // enclosing loop; `continue` is not valid here). The
                    // join node is created lazily: if every case terminates
                    // control flow (e.g. `return` in every case), nothing
                    // ever needs a join node, and creating one eagerly would
                    // leave it unreachable.
                    let idx = self.loop_stack.len();
                    let label = self.pending_label.take();
                    self.loop_stack.push(LoopContext {
                        break_target: None,
                        continue_target: None,
                        label,
                    });

                    let mut cursor = node.walk();
                    let cases: Vec<Node<'_>> = node
                        .children(&mut cursor)
                        .filter(|c| matches!(c.kind(), "communication_case" | "default_case"))
                        .collect();

                    stack.push(Task::AfterSelectCases);
                    stack.push(Task::CaseList {
                        idx,
                        condition_node,
                        cases,
                        pos: 0,
                    });
                }
                Task::AfterSelectCases => {
                    let ctx = self.loop_stack.pop().expect("pushed select context above");
                    self.current_node = ctx.break_target;
                }
                Task::Labeled { node } => {
                    // A label is a jump target reachable via goto. Always
                    // create a node for it so dead code after a
                    // goto/return/break doesn't panic.
                    let label_node = self.cfg.add_node(NodeKind::Statement);
                    if let Some(from) = self.current_node {
                        self.cfg.add_edge(from, label_node);
                    } else {
                        self.cfg.add_edge(self.cfg.entry, label_node);
                    }
                    self.current_node = Some(label_node);

                    // Record the label so the following loop/switch/select
                    // statement can be targeted by a labeled break/continue.
                    self.pending_label = find_label_text(&node, source);

                    // Visit the labeled statement itself (skip the
                    // label_name child)
                    let mut cursor = node.walk();
                    let target = node
                        .children(&mut cursor)
                        .find(|c| c.is_named() && c.kind() != "label_name");

                    stack.push(Task::AfterLabeled);
                    if let Some(target) = target {
                        stack.push(Task::VisitNode { node: target });
                    }
                }
                Task::AfterLabeled => {
                    self.pending_label = None;
                }
            }
        }
    }

    /// Dispatch a single tree-sitter node: compound/control nodes push
    /// further `Task`s (replacing what used to be a native recursive call);
    /// simple nodes are handled inline since they never recurse.
    fn dispatch_node<'a>(&mut self, node: Node<'a>, source: &str, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "if_statement" => stack.push(Task::If { node }),
            "for_statement" => stack.push(Task::For { node }),
            "switch_statement" | "expression_switch_statement" | "type_switch_statement" => {
                // Type switches work similar to regular switches
                stack.push(Task::Switch { node })
            }
            "select_statement" => stack.push(Task::Select { node }),
            "return_statement" => self.visit_return(&node),
            "break_statement" => self.visit_break(&node, source),
            "continue_statement" => self.visit_continue(&node, source),
            "goto_statement" => self.visit_goto(),
            "labeled_statement" => stack.push(Task::Labeled { node }),
            "defer_statement" => self.visit_defer(&node),
            "go_statement" => self.visit_go_statement(&node),
            "expression_statement" => self.visit_expression_statement(&node, source),
            "block" => stack.push(Task::FromBlock { block: node }),
            // tree-sitter-go wraps every sequence of statements (a
            // function/if/for block body, a switch/select case body, etc.)
            // in a statement_list node. Without flattening it here, every
            // statement inside would be treated as one opaque,
            // un-dispatched node - collapsing entire multi-statement blocks
            // into a single generic statement and silently skipping any
            // if/for/switch/etc. nested inside.
            "statement_list" => stack.push(Task::Children {
                children: named_children(&node),
                idx: 0,
            }),
            _ => {
                // Regular statement - add node and continue
                self.visit_simple_statement();
            }
        }
    }

    /// A bare expression statement is a `panic(...)` call or an ordinary
    /// statement; distinguishing the two is real decision logic, not
    /// dispatch, so it lives here rather than inline in `dispatch_node`'s
    /// match.
    fn visit_expression_statement(&mut self, node: &Node, source: &str) {
        if is_panic_call(node, source) {
            self.visit_panic();
        } else {
            self.visit_simple_statement();
        }
    }

    fn visit_simple_statement(&mut self) {
        if let Some(from_node) = self.current_node {
            let stmt_node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, stmt_node);
            self.current_node = Some(stmt_node);
        }
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

/// Collect every named child of `node`, in order (skips braces and other
/// structural/punctuation tokens, same as the old recursive `visit_children`
/// did).
fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.is_named())
        .collect()
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

/// True for switch/type-switch case node kinds (`switch x { ... }` and
/// `switch x.(type) { ... }` share `default_case`; only `expression_case` vs.
/// `type_case` differs between them).
/// Whether a tree-sitter-go node kind is a `switch` case arm
/// (`expression_case`/`type_case`/`default_case`). Used by the `Switch` task
/// to filter case children before handing them to `CaseBody`; select's
/// `communication_case` is a distinct kind and isn't covered here.
fn is_switch_case_kind(kind: &str) -> bool {
    matches!(kind, "expression_case" | "type_case" | "default_case")
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
                source: source.into(),
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
    fn test_go_cfg_builder_control_flow_in_dead_code_does_not_panic() {
        // Regression: control flow after a terminating statement (here rclone's iCloud
        // `Copy`, which returns early and leaves the rest as dead code) used to hit
        // `expect("Current node should exist")`.
        for tail in [
            "if !ok { return nil, err }",
            "for i := 0; i < 3; i++ { println(i) }",
            "switch n { case 1: println(n) }",
            "select { case <-ch: println(n) }",
        ] {
            for term in ["return nil, err", "panic(\"x\")"] {
                let source = format!(
                    "package main\nfunc test(src any, n int, ch chan int) (any, error) {{\n    {term}\n    v, ok := src.(int)\n    _ = v\n    {tail}\n    return nil, nil\n}}\n"
                );
                let mut function = make_test_go_function(&source);
                // The builder locates the function by span start, so point it at `func`.
                function.span =
                    SourceSpan::new(source.find("func").unwrap(), source.len(), 1, 1, 0);
                let cfg = GoCfgBuilder.build(&function);
                assert!(cfg.node_count() > 2, "{term} then {tail}");
            }
        }
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

    #[test]
    fn test_is_switch_case_kind() {
        assert!(is_switch_case_kind("expression_case"));
        assert!(is_switch_case_kind("type_case"));
        assert!(is_switch_case_kind("default_case"));
        // select's case kind is distinct and must not be treated as a switch case.
        assert!(!is_switch_case_kind("communication_case"));
        assert!(!is_switch_case_kind("block"));
    }

    #[test]
    fn test_visit_expression_statement_panic_call_marks_dead_code() {
        // visit_panic (unlike visit_simple_statement) sets current_node to
        // None, so statements after a bare panic() call must be elided from
        // the CFG entirely rather than turned into unreachable nodes.
        let panic_source = r#"
package main
func test() {
    doA()
    panic("boom")
    doB()
    doC()
}
"#;
        let plain_source = r#"
package main
func test() {
    doA()
    doB()
    doC()
}
"#;
        let panic_cfg = build_cfg(panic_source);
        let plain_cfg = build_cfg(plain_source);
        panic_cfg.validate().unwrap();
        plain_cfg.validate().unwrap();

        assert_eq!(
            panic_cfg.node_count(),
            4, // entry, doA, panic, exit — doB/doC are dead code
            "statements after panic() must not become CFG nodes"
        );
        assert_eq!(
            plain_cfg.node_count(),
            5, // entry, doA, doB, doC, exit — all reachable
            "an ordinary call sequence keeps every statement reachable"
        );
    }

    /// Regression test for https://github.com/Stephen-Collins-tech/hotspots/issues/160:
    /// a deeply nested if/else chain used to overflow the calling thread's
    /// stack because the old builder recursed natively per nesting level
    /// (`visit_node` -> `visit_if` -> `build_from_block` -> `visit_children`
    /// -> `visit_node` -> ...).
    ///
    /// tree-sitter-go's own parser (not this builder) has a separate,
    /// unrelated native-recursion limit of its own — confirmed by profiling
    /// `GoParser::parse`/`discover_functions` alone at this depth, which
    /// needs ~5MB of stack regardless of whether a CFG is ever built from
    /// the result. The 6MB stack below is sized for *that* pre-existing
    /// limit, matching the same "give the unrelated parser recursion enough
    /// room, then prove the builder itself needs no more than that" approach
    /// `PR #263` used for `swc_ecma_parser`'s equivalent limit. What this
    /// test actually isolates is the builder: profiling showed the full
    /// parse+discover+build pipeline overflows at the *same* stack sizes as
    /// parse+discover alone (no extra headroom needed for the build step),
    /// proving `build_from_block`'s `run` loop adds no native recursion of
    /// its own on top of the parser's.
    #[test]
    fn test_deeply_nested_if_else_does_not_overflow_and_builds_real_cfg() {
        const DEPTH: usize = 2_000;

        let mut source = String::from("package main\nfunc test(x int) {\n");
        for _ in 0..DEPTH {
            source.push_str("if x > 0 {\n");
        }
        source.push_str("x = 1\n");
        for _ in 0..DEPTH {
            source.push_str("} else {\nx = 2\n}\n");
        }
        source.push('}');

        let node_count = std::thread::Builder::new()
            .stack_size(6 * 1024 * 1024)
            .spawn(move || {
                use crate::language::GoParser;
                let parser = GoParser::new().unwrap();
                let module = parser.parse(&source, "test.go").unwrap();
                let functions = module.discover_functions(0, &source);
                assert_eq!(functions.len(), 1);
                let builder = GoCfgBuilder;
                let cfg = builder.build(&functions[0]);
                cfg.node_count()
            })
            .unwrap()
            .join()
            .unwrap();

        // Each nesting level adds a condition node and at least one branch
        // node; a minimal/degenerate CFG would be far smaller than DEPTH.
        assert!(
            node_count > DEPTH,
            "expected a fully-built CFG with >{DEPTH} nodes, got {node_count}"
        );
    }
}
