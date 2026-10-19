//! CFG builder - constructs CFG from AST
//!
//! Global invariants enforced:
//! - One CFG per function
//! - Deterministic node creation order
//! - Control-relevant statements only
//!
//! Walks the `swc` AST iteratively via an explicit heap-allocated work stack
//! (`Task`, driven by the `run` loop, with results threaded through
//! `self.current_node`) instead of native function-call recursion between
//! `visit_*`/`build_*` methods. This removes the depth bound entirely —
//! nesting depth is now limited by available heap/memory, not by the calling
//! thread's stack — closing the structural gap tracked in
//! https://github.com/Stephen-Collins-tech/hotspots/issues/160 for the
//! shared JS/TS builder (see PR #245 for the equivalent Rust-builder
//! conversion this follows).

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use swc_ecma_ast::*;

/// Build a CFG from a function's AST body
pub fn build_cfg(function: &FunctionNode) -> Cfg {
    let mut builder = CfgBuilder::new();
    // Extract ECMAScript body - this will panic if the function is not ECMAScript
    // This is intentional: only ECMAScript is currently supported
    let block_stmt = function.body.as_ecmascript();
    builder.build_from_body(&block_stmt.stmts);
    builder.cfg
}

/// Context for break/continue target resolution
struct BreakableContext {
    label: Option<String>,
    /// `Some` when the join node has already been created (condition-guarded loops and
    /// switch), `None` when it must be created on the first `break` statement
    /// (do-while, infinite `for` loops).
    break_target: Option<NodeId>,
    /// None for switch (not continuable), Some for loops
    continue_target: Option<NodeId>,
}

/// Pending unit of work on the explicit stack.
///
/// The "visit"-shaped variants (`If`, `While`, `Try`, ...) mirror what the
/// old recursive `visit_*`/`build_*` methods did on the way down the call
/// stack; the "After*" variants mirror what they did with the recursive
/// result on the way back up, now driven by a loop instead of native stack
/// unwinding. Each variant borrows from the same `'a` AST tree rooted at the
/// function body, which outlives the whole build. The "result" of whatever
/// nested work just ran is read from `self.current_node` (exactly as the
/// recursive methods used it before), so — unlike a typical CPS stack — no
/// separate result-passing field is needed.
enum Task<'a> {
    /// Sequential statement list (block body, case body, ...).
    Stmts {
        stmts: &'a [Stmt],
        idx: usize,
    },
    Stmt {
        stmt: &'a Stmt,
    },
    /// Clears `pending_label` after a labeled statement's body has been visited.
    ClearLabel,
    /// Function/try/catch/finally body: sequential statements, then force an
    /// edge to `exit` from the last node if one doesn't already exist.
    FromBody {
        stmts: &'a [Stmt],
    },
    AfterFromBody,
    If {
        if_stmt: &'a IfStmt,
    },
    AfterIfThen {
        condition_node: NodeId,
        alt: Option<&'a Stmt>,
    },
    AfterIfElse {
        then_end: Option<NodeId>,
    },
    While {
        while_stmt: &'a WhileStmt,
    },
    AfterWhileBody {
        header_node: NodeId,
        join_node: NodeId,
        body_start: NodeId,
    },
    DoWhile {
        do_while_stmt: &'a DoWhileStmt,
    },
    AfterDoWhileBody {
        header_node: NodeId,
        body_start: NodeId,
    },
    For {
        for_stmt: &'a ForStmt,
    },
    AfterForBody {
        for_stmt: &'a ForStmt,
        header_node: NodeId,
        body_start: NodeId,
    },
    ForIn {
        for_in_stmt: &'a ForInStmt,
    },
    AfterForInBody {
        header_node: NodeId,
        join_node: NodeId,
        body_start: NodeId,
    },
    ForOf {
        for_of_stmt: &'a ForOfStmt,
    },
    AfterForOfBody {
        header_node: NodeId,
        join_node: NodeId,
        body_start: NodeId,
    },
    Switch {
        switch_stmt: &'a SwitchStmt,
    },
    SwitchCases {
        switch_node: NodeId,
        cases: std::slice::Iter<'a, SwitchCase>,
        prev_case_end: Option<NodeId>,
    },
    AfterSwitchCaseBody {
        switch_node: NodeId,
        cases: std::slice::Iter<'a, SwitchCase>,
    },
    Try {
        try_stmt: &'a TryStmt,
    },
    AfterTryBlock {
        try_stmt: &'a TryStmt,
        try_start: NodeId,
    },
    AfterCatchBlock {
        try_stmt: &'a TryStmt,
        try_start: NodeId,
        try_end: NodeId,
        try_completed: bool,
    },
    AfterTryCatch {
        try_stmt: &'a TryStmt,
        try_start: NodeId,
        try_end: NodeId,
        try_completed: bool,
        catch_end: Option<NodeId>,
    },
    AfterFinallyBlock,
}

/// Builder for constructing CFG from AST
struct CfgBuilder {
    cfg: Cfg,
    current_node: Option<NodeId>,
    /// Stack of enclosing loop/switch contexts for break/continue routing
    breakable_stack: Vec<BreakableContext>,
    /// Label from a LabeledStmt, consumed by the next loop/switch visitor
    pending_label: Option<String>,
}

impl CfgBuilder {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;

        CfgBuilder {
            cfg,
            current_node: Some(entry),
            breakable_stack: Vec::new(),
            pending_label: None,
        }
    }

    /// Take the pending label (if any) for the next loop/switch context
    fn take_label(&mut self) -> Option<String> {
        self.pending_label.take()
    }

    /// Return the break-target join node for the given stack index, creating it
    /// lazily if it has not been created yet (do-while, infinite for).
    fn get_or_create_break_target(&mut self, idx: usize) -> NodeId {
        if let Some(target) = self.breakable_stack[idx].break_target {
            return target;
        }
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.breakable_stack[idx].break_target = Some(join_node);
        join_node
    }

    /// Build CFG from a sequential statement list, forcing a trailing edge to
    /// `exit` from the last node if one doesn't already exist. Used for the
    /// function body and for try/catch/finally bodies.
    fn build_from_body(&mut self, stmts: &[Stmt]) {
        let mut stack: Vec<Task<'_>> = vec![Task::FromBody { stmts }];
        self.run(&mut stack);
    }

    /// Drive the explicit work-stack loop. Pushing a `Task` here replaces what
    /// used to be a native recursive call; the "After*" variants replace what
    /// used to run after that recursive call returned.
    fn run(&mut self, stack: &mut Vec<Task<'_>>) {
        while let Some(task) = stack.pop() {
            match task {
                Task::Stmts { stmts, idx } => {
                    if idx < stmts.len() {
                        stack.push(Task::Stmts {
                            stmts,
                            idx: idx + 1,
                        });
                        stack.push(Task::Stmt { stmt: &stmts[idx] });
                    }
                }
                Task::Stmt { stmt } => self.dispatch_stmt(stmt, stack),
                Task::ClearLabel => {
                    self.pending_label = None;
                }
                Task::FromBody { stmts } => {
                    stack.push(Task::AfterFromBody);
                    stack.push(Task::Stmts { stmts, idx: 0 });
                }
                Task::AfterFromBody => {
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
                Task::If { if_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };

                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, condition_node);

                    let then_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, then_start);
                    self.current_node = Some(then_start);

                    stack.push(Task::AfterIfThen {
                        condition_node,
                        alt: if_stmt.alt.as_deref(),
                    });
                    stack.push(Task::Stmt {
                        stmt: &if_stmt.cons,
                    });
                }
                Task::AfterIfThen {
                    condition_node,
                    alt,
                } => {
                    let then_end = self.current_node;

                    if let Some(alt) = alt {
                        let else_start = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(condition_node, else_start);
                        self.current_node = Some(else_start);

                        stack.push(Task::AfterIfElse { then_end });
                        stack.push(Task::Stmt { stmt: alt });
                    } else {
                        // No else: condition false-edge always reaches join
                        let join_node = self.cfg.add_node(NodeKind::Join);
                        self.cfg.add_edge(condition_node, join_node);
                        if let Some(t) = then_end {
                            self.cfg.add_edge(t, join_node);
                        }
                        self.current_node = Some(join_node);
                    }
                }
                Task::AfterIfElse { then_end } => {
                    let else_end = self.current_node;

                    if then_end.is_some() || else_end.is_some() {
                        let join_node = self.cfg.add_node(NodeKind::Join);
                        if let Some(t) = then_end {
                            self.cfg.add_edge(t, join_node);
                        }
                        if let Some(e) = else_end {
                            self.cfg.add_edge(e, join_node);
                        }
                        self.current_node = Some(join_node);
                    } else {
                        self.current_node = None; // both branches terminate
                    }
                }
                Task::While { while_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let label = self.take_label();

                    let header_node = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, header_node);

                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(header_node, condition_node);

                    // The condition always provides a false-branch edge to join, so
                    // eager creation is safe — join is always reachable regardless
                    // of the body.
                    let join_node = self.cfg.add_node(NodeKind::Join);
                    self.cfg.add_edge(condition_node, join_node);

                    self.breakable_stack.push(BreakableContext {
                        label,
                        break_target: Some(join_node),
                        continue_target: Some(header_node),
                    });

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, body_start);
                    self.current_node = Some(body_start);

                    stack.push(Task::AfterWhileBody {
                        header_node,
                        join_node,
                        body_start,
                    });
                    stack.push(Task::Stmt {
                        stmt: &while_stmt.body,
                    });
                }
                Task::AfterWhileBody {
                    header_node,
                    join_node,
                    body_start,
                } => {
                    let body_end = self.current_node.unwrap_or(body_start);
                    self.breakable_stack.pop();

                    if self.current_node.is_some() {
                        self.cfg.add_edge(body_end, header_node);
                    }
                    self.current_node = Some(join_node);
                }
                Task::DoWhile { do_while_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let label = self.take_label();

                    let header_node = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, header_node);

                    // Unlike while/for, do-while has no pre-body condition that
                    // could provide a guaranteed edge to the join node. The join
                    // is therefore created lazily: only when a `break` or the
                    // post-body condition needs it.
                    self.breakable_stack.push(BreakableContext {
                        label,
                        break_target: None,
                        continue_target: Some(header_node),
                    });

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(header_node, body_start);
                    self.current_node = Some(body_start);

                    stack.push(Task::AfterDoWhileBody {
                        header_node,
                        body_start,
                    });
                    stack.push(Task::Stmt {
                        stmt: &do_while_stmt.body,
                    });
                }
                Task::AfterDoWhileBody {
                    header_node,
                    body_start,
                } => {
                    let body_end = self.current_node.unwrap_or(body_start);
                    let body_completed = self.current_node.is_some();

                    let ctx = self.breakable_stack.pop().unwrap();
                    let lazy_join = ctx.break_target;

                    if body_completed {
                        let join_node =
                            lazy_join.unwrap_or_else(|| self.cfg.add_node(NodeKind::Join));
                        let condition_node = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(body_end, condition_node);
                        self.cfg.add_edge(condition_node, header_node);
                        self.cfg.add_edge(condition_node, join_node);
                        self.current_node = Some(join_node);
                    } else if let Some(join_node) = lazy_join {
                        self.current_node = Some(join_node);
                    } else {
                        self.current_node = None;
                    }
                }
                Task::For { for_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let label = self.take_label();

                    let init_end = if for_stmt.init.is_some() {
                        let node = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(from_node, node);
                        node
                    } else {
                        from_node
                    };

                    let header_node = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(init_end, header_node);

                    let (condition_node, initial_break_target) = if for_stmt.test.is_some() {
                        let cnode = self.cfg.add_node(NodeKind::Condition);
                        self.cfg.add_edge(header_node, cnode);
                        let join = self.cfg.add_node(NodeKind::Join);
                        self.cfg.add_edge(cnode, join);
                        (cnode, Some(join))
                    } else {
                        (header_node, None)
                    };

                    self.breakable_stack.push(BreakableContext {
                        label,
                        break_target: initial_break_target,
                        continue_target: Some(header_node),
                    });

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, body_start);
                    self.current_node = Some(body_start);

                    stack.push(Task::AfterForBody {
                        for_stmt,
                        header_node,
                        body_start,
                    });
                    stack.push(Task::Stmt {
                        stmt: &for_stmt.body,
                    });
                }
                Task::AfterForBody {
                    for_stmt,
                    header_node,
                    body_start,
                } => {
                    let mut body_end = self.current_node.unwrap_or(body_start);
                    let ctx = self.breakable_stack.pop().unwrap();

                    if self.current_node.is_some() {
                        if for_stmt.update.is_some() {
                            let update_node = self.cfg.add_node(NodeKind::Statement);
                            self.cfg.add_edge(body_end, update_node);
                            body_end = update_node;
                        }
                        self.cfg.add_edge(body_end, header_node);
                    }

                    if let Some(join_node) = ctx.break_target {
                        self.current_node = Some(join_node);
                    } else {
                        self.current_node = None;
                    }
                }
                Task::ForIn { for_in_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let label = self.take_label();

                    let header_node = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, header_node);

                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(header_node, condition_node);

                    // Create join node BEFORE body so break can target it
                    let join_node = self.cfg.add_node(NodeKind::Join);
                    self.cfg.add_edge(condition_node, join_node);

                    self.breakable_stack.push(BreakableContext {
                        label,
                        break_target: Some(join_node),
                        continue_target: Some(header_node),
                    });

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, body_start);
                    self.current_node = Some(body_start);

                    stack.push(Task::AfterForInBody {
                        header_node,
                        join_node,
                        body_start,
                    });
                    stack.push(Task::Stmt {
                        stmt: &for_in_stmt.body,
                    });
                }
                Task::AfterForInBody {
                    header_node,
                    join_node,
                    body_start,
                } => {
                    let body_end = self.current_node.unwrap_or(body_start);
                    self.breakable_stack.pop();

                    if self.current_node.is_some() {
                        self.cfg.add_edge(body_end, header_node);
                    }
                    self.current_node = Some(join_node);
                }
                Task::ForOf { for_of_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let label = self.take_label();

                    let header_node = self.cfg.add_node(NodeKind::LoopHeader);
                    self.cfg.add_edge(from_node, header_node);

                    let condition_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(header_node, condition_node);

                    // Create join node BEFORE body so break can target it
                    let join_node = self.cfg.add_node(NodeKind::Join);
                    self.cfg.add_edge(condition_node, join_node);

                    self.breakable_stack.push(BreakableContext {
                        label,
                        break_target: Some(join_node),
                        continue_target: Some(header_node),
                    });

                    let body_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(condition_node, body_start);
                    self.current_node = Some(body_start);

                    stack.push(Task::AfterForOfBody {
                        header_node,
                        join_node,
                        body_start,
                    });
                    stack.push(Task::Stmt {
                        stmt: &for_of_stmt.body,
                    });
                }
                Task::AfterForOfBody {
                    header_node,
                    join_node,
                    body_start,
                } => {
                    let body_end = self.current_node.unwrap_or(body_start);
                    self.breakable_stack.pop();

                    if self.current_node.is_some() {
                        self.cfg.add_edge(body_end, header_node);
                    }
                    self.current_node = Some(join_node);
                }
                Task::Switch { switch_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };
                    let label = self.take_label();

                    let switch_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(from_node, switch_node);

                    // The join node is created lazily: it comes into existence on
                    // the first `break` statement or when a case falls through
                    // past the end.
                    self.breakable_stack.push(BreakableContext {
                        label,
                        break_target: None,
                        continue_target: None, // switch is not continuable
                    });

                    stack.push(Task::SwitchCases {
                        switch_node,
                        cases: switch_stmt.cases.iter(),
                        prev_case_end: None,
                    });
                }
                Task::SwitchCases {
                    switch_node,
                    mut cases,
                    prev_case_end,
                } => {
                    if let Some(case) = cases.next() {
                        let case_node = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(switch_node, case_node);

                        // Fallthrough from previous case if it didn't break/return/throw
                        if let Some(prev_end) = prev_case_end {
                            self.cfg.add_edge(prev_end, case_node);
                        }

                        self.current_node = Some(case_node);
                        stack.push(Task::AfterSwitchCaseBody { switch_node, cases });
                        stack.push(Task::Stmts {
                            stmts: &case.cons,
                            idx: 0,
                        });
                    } else {
                        let ctx = self.breakable_stack.pop().unwrap();
                        let lazy_join = ctx.break_target;

                        if let Some(last_end) = prev_case_end {
                            // Last case fell through — wire it to the (possibly
                            // new) join node.
                            let join_node =
                                lazy_join.unwrap_or_else(|| self.cfg.add_node(NodeKind::Join));
                            self.cfg.add_edge(last_end, join_node);
                            self.current_node = Some(join_node);
                        } else if let Some(join_node) = lazy_join {
                            self.current_node = Some(join_node);
                        } else {
                            // All cases terminated with no break and no
                            // fallthrough. Code after the switch is dead.
                            self.current_node = None;
                        }
                    }
                }
                Task::AfterSwitchCaseBody { switch_node, cases } => {
                    // Track end of case for fallthrough to next case. If
                    // current_node is None (break/return/throw), no fallthrough.
                    let prev_case_end = self.current_node;
                    stack.push(Task::SwitchCases {
                        switch_node,
                        cases,
                        prev_case_end,
                    });
                }
                Task::Try { try_stmt } => {
                    let Some(from_node) = self.current_node else {
                        continue;
                    };

                    let try_start = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(from_node, try_start);
                    self.current_node = Some(try_start);

                    stack.push(Task::AfterTryBlock {
                        try_stmt,
                        try_start,
                    });
                    stack.push(Task::FromBody {
                        stmts: &try_stmt.block.stmts,
                    });
                }
                Task::AfterTryBlock {
                    try_stmt,
                    try_start,
                } => {
                    let try_end = self.current_node.unwrap_or(try_start);
                    let try_completed = self.current_node.is_some();

                    if let Some(handler) = &try_stmt.handler {
                        let catch_start = self.cfg.add_node(NodeKind::Statement);
                        self.cfg.add_edge(try_start, catch_start);
                        self.current_node = Some(catch_start);

                        stack.push(Task::AfterCatchBlock {
                            try_stmt,
                            try_start,
                            try_end,
                            try_completed,
                        });
                        stack.push(Task::FromBody {
                            stmts: &handler.body.stmts,
                        });
                    } else {
                        stack.push(Task::AfterTryCatch {
                            try_stmt,
                            try_start,
                            try_end,
                            try_completed,
                            catch_end: None,
                        });
                    }
                }
                Task::AfterCatchBlock {
                    try_stmt,
                    try_start,
                    try_end,
                    try_completed,
                } => {
                    let catch_end = self.current_node;
                    stack.push(Task::AfterTryCatch {
                        try_stmt,
                        try_start,
                        try_end,
                        try_completed,
                        catch_end,
                    });
                }
                Task::AfterTryCatch {
                    try_stmt,
                    try_start,
                    try_end,
                    try_completed,
                    catch_end,
                } => {
                    let has_handler = try_stmt.handler.is_some();

                    if let Some(finally_block) = &try_stmt.finalizer {
                        let finally_start = self.cfg.add_node(NodeKind::Statement);
                        if try_completed {
                            self.cfg.add_edge(try_end, finally_start);
                        }
                        if let Some(catch) = catch_end {
                            if catch != self.cfg.exit {
                                self.cfg.add_edge(catch, finally_start);
                            }
                        }
                        // Fallback edge only when finally_start would otherwise
                        // have zero predecessors: try always terminates AND
                        // catch always terminates (no handler, or handler throws).
                        if !try_completed && catch_end.is_none() {
                            self.cfg.add_edge(try_start, finally_start);
                        }
                        self.current_node = Some(finally_start);

                        stack.push(Task::AfterFinallyBlock);
                        stack.push(Task::FromBody {
                            stmts: &finally_block.stmts,
                        });
                    } else {
                        let catch_completed =
                            catch_end.map(|c| c != self.cfg.exit).unwrap_or(false);
                        if try_completed || catch_completed {
                            let join_node = self.cfg.add_node(NodeKind::Join);
                            if try_completed {
                                self.cfg.add_edge(try_end, join_node);
                            }
                            if let Some(catch) = catch_end {
                                if catch != self.cfg.exit {
                                    self.cfg.add_edge(catch, join_node);
                                }
                            }
                            if !has_handler {
                                self.cfg.add_edge(try_start, self.cfg.exit);
                            }
                            self.current_node = Some(join_node);
                        } else {
                            if !has_handler {
                                self.cfg.add_edge(try_start, self.cfg.exit);
                            }
                            self.current_node = None;
                        }
                    }
                }
                Task::AfterFinallyBlock => {
                    if let Some(finally_end) = self.current_node {
                        let join_node = self.cfg.add_node(NodeKind::Join);
                        self.cfg.add_edge(finally_end, join_node);
                        self.current_node = Some(join_node);
                    }
                    // else: finally always terminates; current_node stays None
                }
            }
        }
    }

    /// Dispatch a single statement: compound/control statements push further
    /// `Task`s (replacing what used to be a native recursive call); simple
    /// statements are handled inline since they never recurse.
    fn dispatch_stmt<'a>(&mut self, stmt: &'a Stmt, stack: &mut Vec<Task<'a>>) {
        match stmt {
            Stmt::Labeled(labeled) => {
                // Store label for the next loop/switch to consume
                self.pending_label = Some(labeled.label.sym.to_string());
                stack.push(Task::ClearLabel);
                stack.push(Task::Stmt {
                    stmt: &labeled.body,
                });
            }
            Stmt::If(if_stmt) => stack.push(Task::If { if_stmt }),
            Stmt::While(while_stmt) => stack.push(Task::While { while_stmt }),
            Stmt::DoWhile(do_while_stmt) => stack.push(Task::DoWhile { do_while_stmt }),
            Stmt::For(for_stmt) => stack.push(Task::For { for_stmt }),
            Stmt::ForIn(for_in_stmt) => stack.push(Task::ForIn { for_in_stmt }),
            Stmt::ForOf(for_of_stmt) => stack.push(Task::ForOf { for_of_stmt }),
            Stmt::Switch(switch_stmt) => stack.push(Task::Switch { switch_stmt }),
            Stmt::Return(return_stmt) => self.visit_return(return_stmt),
            Stmt::Throw(throw_stmt) => self.visit_throw(throw_stmt),
            Stmt::Try(try_stmt) => stack.push(Task::Try { try_stmt }),
            Stmt::Break(break_stmt) => self.visit_break(break_stmt),
            Stmt::Continue(continue_stmt) => self.visit_continue(continue_stmt),
            Stmt::Block(block_stmt) => {
                // Nested blocks - visit statements sequentially
                stack.push(Task::Stmts {
                    stmts: &block_stmt.stmts,
                    idx: 0,
                });
            }
            _ => {
                // Control-relevant statement - add node and continue
                if let Some(from_node) = self.current_node {
                    let stmt_node = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(from_node, stmt_node);
                    self.current_node = Some(stmt_node);
                }
            }
        }
    }

    fn visit_return(&mut self, _return_stmt: &ReturnStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };

        // Return statement - edge directly to exit
        self.cfg.add_edge(from_node, self.cfg.exit);

        // No further execution after return
        self.current_node = None;
    }

    fn visit_throw(&mut self, _throw_stmt: &ThrowStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };

        // Throw statement - edge directly to exit
        self.cfg.add_edge(from_node, self.cfg.exit);

        // No further execution after throw
        self.current_node = None;
    }

    fn visit_break(&mut self, break_stmt: &BreakStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let target = if let Some(label) = &break_stmt.label {
            // Labeled break: find the innermost context with this label
            let idx = self
                .breakable_stack
                .iter()
                .rposition(|ctx| ctx.label.as_deref() == Some(&*label.sym));
            idx.map(|i| self.get_or_create_break_target(i))
        } else {
            // Unlabeled break: find innermost breakable context (loop or switch)
            let idx = self.breakable_stack.len().checked_sub(1);
            idx.map(|i| self.get_or_create_break_target(i))
        };

        if let Some(target) = target {
            self.cfg.add_edge(from_node, target);
        } else {
            // No enclosing breakable context (shouldn't happen in valid JS/TS)
            self.cfg.add_edge(from_node, self.cfg.exit);
        }

        self.current_node = None;
    }

    fn visit_continue(&mut self, continue_stmt: &ContinueStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let target = if let Some(label) = &continue_stmt.label {
            // Labeled continue: find the matching labeled loop
            self.breakable_stack
                .iter()
                .rev()
                .find(|ctx| {
                    ctx.label.as_deref() == Some(&*label.sym) && ctx.continue_target.is_some()
                })
                .and_then(|ctx| ctx.continue_target)
        } else {
            // Unlabeled continue: find innermost loop (must have continue_target)
            self.breakable_stack
                .iter()
                .rev()
                .find(|ctx| ctx.continue_target.is_some())
                .and_then(|ctx| ctx.continue_target)
        };

        if let Some(target) = target {
            self.cfg.add_edge(from_node, target);
        } else {
            // No enclosing loop (shouldn't happen in valid JS/TS)
            self.cfg.add_edge(from_node, self.cfg.exit);
        }

        self.current_node = None;
    }
}

impl Default for CfgBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::discover_functions;
    use crate::parser::parse_source;
    use swc_common::{sync::Lrc, SourceMap};

    /// Helper: parse source, discover functions, build CFG for the first function
    fn build_cfg_for(source: &str) -> Cfg {
        let sm = Lrc::new(SourceMap::default());
        let module = parse_source(source, &sm, "test.ts").unwrap();
        let functions = discover_functions(&module, 0, source, &sm);
        assert!(!functions.is_empty(), "Expected at least one function");
        build_cfg(&functions[0])
    }

    #[test]
    fn test_break_routes_to_loop_join() {
        let cfg = build_cfg_for("function f() { while (true) { break; } }");
        cfg.validate().expect("CFG should be valid");

        // break should NOT create an edge to exit (NodeId(1))
        // It should route to the loop's join node
        let break_to_exit = cfg.edges.iter().any(|e| {
            e.to == cfg.exit
                && e.from != cfg.entry
                && !matches!(
                    cfg.nodes[e.from.0].kind,
                    NodeKind::Statement | NodeKind::Join
                )
        });
        // The only edge to exit should be from the join node (normal flow)
        // or from the final implicit connection in build_from_body
        assert!(
            !break_to_exit || cfg.edges.iter().filter(|e| e.to == cfg.exit).count() <= 2,
            "break should not create extra edges to exit"
        );
    }

    #[test]
    fn test_continue_routes_to_loop_header() {
        let cfg = build_cfg_for("function f() { while (true) { continue; } }");
        cfg.validate().expect("CFG should be valid");

        // continue should create an edge to a LoopHeader node
        let has_edge_to_header = cfg.edges.iter().any(|e| {
            matches!(cfg.nodes[e.to.0].kind, NodeKind::LoopHeader)
                && e.from != cfg.entry
                && !matches!(
                    cfg.nodes[e.from.0].kind,
                    NodeKind::Entry | NodeKind::LoopHeader | NodeKind::Condition
                )
        });
        assert!(has_edge_to_header, "continue should route to loop header");
    }

    #[test]
    fn test_labeled_break_routes_to_outer_loop() {
        let cfg =
            build_cfg_for("function f() { outer: while (true) { while (true) { break outer; } } }");
        cfg.validate().expect("CFG should be valid");
    }

    #[test]
    fn test_labeled_continue_routes_to_outer_header() {
        let cfg = build_cfg_for(
            "function f() { outer: while (true) { while (true) { continue outer; } } }",
        );
        cfg.validate().expect("CFG should be valid");
    }

    #[test]
    fn test_switch_break_routes_to_switch_join() {
        let cfg =
            build_cfg_for("function f(x: number) { switch(x) { case 1: break; case 2: break; } }");
        cfg.validate().expect("CFG should be valid");

        // break in switch should NOT route to cfg.exit
        // Count edges to exit - should only be the final flow-to-exit
        let exit_edges: Vec<_> = cfg.edges.iter().filter(|e| e.to == cfg.exit).collect();
        // Only the join node should flow to exit (via build_from_body)
        assert!(
            exit_edges.len() <= 1,
            "switch breaks should route to join, not exit. Exit edges: {:?}",
            exit_edges
        );
    }

    #[test]
    fn test_nested_loop_break_targets_inner() {
        let cfg = build_cfg_for(
            "function f() { for (let i = 0; i < 10; i++) { for (let j = 0; j < 10; j++) { break; } } }",
        );
        cfg.validate().expect("CFG should be valid");
    }

    #[test]
    fn test_for_of_with_break_and_continue() {
        let cfg = build_cfg_for(
            r#"
            function f(arr: number[]) {
                let sum = 0;
                for (const item of arr) {
                    if (item < 0) { break; }
                    if (item > 100) { continue; }
                    sum += item;
                }
                return sum;
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    // --- regression tests for the bugs fixed in this patch ---

    /// do-while whose body always returns: body terminates, no break is executed,
    /// so the join node must never be created (no orphaned unreachable node).
    #[test]
    fn test_do_while_body_always_returns() {
        let cfg = build_cfg_for(
            r#"
            function f(x: number): number {
                do {
                    return x * 2;
                } while (x > 0);
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// do-while with a break inside: join node must be created and reachable.
    #[test]
    fn test_do_while_with_break() {
        let cfg = build_cfg_for(
            r#"
            function f(x: number): number {
                do {
                    if (x < 0) { break; }
                    x--;
                } while (x > 0);
                return x;
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// do-while that completes normally (no early exit): standard case.
    #[test]
    fn test_do_while_normal_completion() {
        let cfg = build_cfg_for(
            r#"
            function f(x: number): number {
                do {
                    x--;
                } while (x > 0);
                return x;
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// Infinite for-loop whose body always returns: no join node should be
    /// created, so no orphaned unreachable node.
    #[test]
    fn test_infinite_for_body_always_returns() {
        let cfg = build_cfg_for(
            r#"
            function f(x: number): number {
                for (;;) {
                    return x;
                }
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// Infinite for-loop with a break: join node is created and reachable.
    #[test]
    fn test_infinite_for_with_break() {
        let cfg = build_cfg_for(
            r#"
            function f(x: number): number {
                for (;;) {
                    if (x <= 0) { break; }
                    x--;
                }
                return x;
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// Switch where every case terminates via return: join node must still be
    /// reachable (via the switch_node → join_node "no match" edge).
    #[test]
    fn test_switch_all_cases_return() {
        let cfg = build_cfg_for(
            r#"
            function f(x: number): string {
                switch (x) {
                    case 1: return "one";
                    case 2: return "two";
                    default: return "other";
                }
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// try { return } catch { throw } finally { stmt } — the TanStack/query
    /// pattern: try returns, catch re-throws, finally falls through.
    /// finally_start must be reachable (via try_start → finally_start) even
    /// though both the try and catch bodies terminate.
    #[test]
    fn test_try_return_catch_throw_finally_reachable() {
        let cfg = build_cfg_for(
            r#"
            async function execute(x: number): Promise<number> {
                try {
                    return x * 2;
                } catch (error) {
                    throw error;
                } finally {
                    cleanup();
                }
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// Dead code after a return inside an if-branch must not cause a panic.
    /// The if/while/for in unreachable positions should be silently skipped.
    #[test]
    fn test_dead_code_after_return_no_panic() {
        // The second `if` statement is dead code — current_node is None when we
        // reach it. The builder must not panic.
        let cfg = build_cfg_for(
            r#"
            function f(x: number): number {
                if (x > 0) {
                    return x;
                }
                throw new Error("negative");
                if (x < 0) { return -x; }
            }
        "#,
        );
        cfg.validate().expect("CFG should be valid");
    }

    /// Regression test for https://github.com/Stephen-Collins-tech/hotspots/issues/237:
    /// a deeply nested if/else chain used to overflow the calling thread's
    /// stack because the old builder recursed natively per nesting level
    /// (`visit_stmt` -> `visit_if` -> `visit_stmt` -> ...). Build the nested
    /// `IfStmt` AST directly rather than parsing deeply-nested source text:
    /// `swc_ecma_parser` has its own separate, unrelated recursion limit, and
    /// parsing deep source would end up testing the parser's stack depth
    /// instead of this builder's. Run on a small (256KB) thread stack to
    /// prove no native recursion remains in `build_cfg`, and assert on node
    /// count to prove a real, fully-built CFG came out — not a truncated or
    /// degenerate one.
    #[test]
    fn test_deeply_nested_if_else_does_not_overflow_and_builds_real_cfg() {
        const DEPTH: usize = 50_000;

        fn nested_if(depth: usize) -> Stmt {
            let mut body = Stmt::Break(BreakStmt {
                span: Default::default(),
                label: None,
            });
            for _ in 0..depth {
                let cond = Expr::Ident(Ident::new(
                    "x".into(),
                    Default::default(),
                    Default::default(),
                ));
                let if_stmt = IfStmt {
                    span: Default::default(),
                    test: Box::new(cond),
                    cons: Box::new(body),
                    alt: Some(Box::new(Stmt::Expr(ExprStmt {
                        span: Default::default(),
                        expr: Box::new(Expr::Ident(Ident::new(
                            "y".into(),
                            Default::default(),
                            Default::default(),
                        ))),
                    }))),
                };
                body = Stmt::If(if_stmt);
            }
            body
        }

        let node_count = std::thread::Builder::new()
            .stack_size(256 * 1024) // far below what native recursion of this depth would need
            .spawn(move || {
                let stmt = nested_if(DEPTH);
                let mut builder = CfgBuilder::new();
                builder.build_from_body(std::slice::from_ref(&stmt));
                let node_count = builder.cfg.node_count();
                // `Stmt`'s generated `Drop` glue recurses per nesting level,
                // same as `syn::Block`'s does in the equivalent Rust-builder
                // test (PR #245) — that's a property of the AST's nested-Box
                // shape, not of this builder. Skip it so this test isolates
                // what actually changed: `build_from_body`/`run` no longer
                // recurse natively regardless of nesting depth.
                std::mem::forget(stmt);
                node_count
            })
            .unwrap()
            .join()
            .unwrap();

        // Each nesting level adds a condition node and at least one branch
        // node; a minimal/degenerate CFG would be just entry+exit (2 nodes).
        assert!(
            node_count > DEPTH,
            "expected a fully-built CFG with >{DEPTH} nodes, got {node_count}"
        );
    }
}
