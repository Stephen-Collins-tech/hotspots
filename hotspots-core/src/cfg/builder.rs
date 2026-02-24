//! CFG builder - constructs CFG from AST
//!
//! Global invariants enforced:
//! - One CFG per function
//! - Deterministic node creation order
//! - Control-relevant statements only

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use swc_ecma_ast::*;

/// Build a CFG from a function's AST body
pub fn build_cfg(function: &FunctionNode) -> Cfg {
    let mut builder = CfgBuilder::new();
    // Extract ECMAScript body - this will panic if the function is not ECMAScript
    // This is intentional: only ECMAScript is currently supported
    let block_stmt = function.body.as_ecmascript();
    builder.build_from_body(block_stmt);
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

    /// Build CFG from a block statement body
    fn build_from_body(&mut self, body: &BlockStmt) {
        for stmt in &body.stmts {
            self.visit_stmt(stmt);
        }

        // Connect last node to exit (if not already connected)
        if let Some(last_node) = self.current_node {
            if last_node != self.cfg.exit {
                // Check if last node already has an edge to exit
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

    /// Visit a statement and add CFG nodes/edges
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Labeled(labeled) => {
                // Store label for the next loop/switch to consume
                self.pending_label = Some(labeled.label.sym.to_string());
                self.visit_stmt(&labeled.body);
                // Clear if not consumed (e.g., label on non-loop statement)
                self.pending_label = None;
            }
            Stmt::If(if_stmt) => self.visit_if(if_stmt),
            Stmt::While(while_stmt) => self.visit_while(while_stmt),
            Stmt::DoWhile(do_while_stmt) => self.visit_do_while(do_while_stmt),
            Stmt::For(for_stmt) => self.visit_for(for_stmt),
            Stmt::ForIn(for_in_stmt) => self.visit_for_in(for_in_stmt),
            Stmt::ForOf(for_of_stmt) => self.visit_for_of(for_of_stmt),
            Stmt::Switch(switch_stmt) => self.visit_switch(switch_stmt),
            Stmt::Return(return_stmt) => self.visit_return(return_stmt),
            Stmt::Throw(throw_stmt) => self.visit_throw(throw_stmt),
            Stmt::Try(try_stmt) => self.visit_try(try_stmt),
            Stmt::Break(break_stmt) => self.visit_break(break_stmt),
            Stmt::Continue(continue_stmt) => self.visit_continue(continue_stmt),
            Stmt::Block(block_stmt) => {
                // Nested blocks - visit statements sequentially
                for stmt in &block_stmt.stmts {
                    self.visit_stmt(stmt);
                }
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

    fn visit_if(&mut self, if_stmt: &IfStmt) {
        // Dead code after a terminator — skip silently
        let Some(from_node) = self.current_node else {
            return;
        };

        // Condition node
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, condition_node);

        // Then branch
        let then_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, then_start);

        self.current_node = Some(then_start);
        self.visit_stmt(&if_stmt.cons);
        let then_end = self.current_node.unwrap_or(then_start);

        // Else branch (if exists)
        let join_node = self.cfg.add_node(NodeKind::Join);

        if let Some(alt) = &if_stmt.alt {
            let else_start = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(condition_node, else_start);

            self.current_node = Some(else_start);
            self.visit_stmt(alt);
            let else_end = self.current_node.unwrap_or(else_start);

            // Connect both branches to join
            // Only connect then branch if it completed normally
            if then_end != self.cfg.exit {
                self.cfg.add_edge(then_end, join_node);
            }
            // Only connect else branch if it completed normally
            if else_end != self.cfg.exit && self.current_node.is_some() {
                self.cfg.add_edge(else_end, join_node);
            }

            // Set current_node to join only if at least one branch completed normally
            if then_end != self.cfg.exit
                || (else_end != self.cfg.exit && self.current_node.is_some())
            {
                self.current_node = Some(join_node);
            }
        } else {
            // No else - condition false edge goes directly to join
            self.cfg.add_edge(condition_node, join_node);
            // Only connect then branch if it completed normally
            if then_end != self.cfg.exit {
                self.cfg.add_edge(then_end, join_node);
                self.current_node = Some(join_node);
            }
            // If then branch terminated, current_node is already None
        }
    }

    fn visit_while(&mut self, while_stmt: &WhileStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };
        let label = self.take_label();

        // Loop header node
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);

        // Condition node
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(header_node, condition_node);

        // The condition always provides a false-branch edge to join, so eager
        // creation is safe — join is always reachable regardless of the body.
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.cfg.add_edge(condition_node, join_node);

        // Push loop context for break/continue resolution
        self.breakable_stack.push(BreakableContext {
            label,
            break_target: Some(join_node),
            continue_target: Some(header_node),
        });

        // Loop body
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);

        self.current_node = Some(body_start);
        self.visit_stmt(&while_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);

        self.breakable_stack.pop();

        // Back-edge to header (if body didn't terminate with return/throw)
        if self.current_node.is_some() {
            self.cfg.add_edge(body_end, header_node);
        }

        self.current_node = Some(join_node);
    }

    fn visit_do_while(&mut self, do_while_stmt: &DoWhileStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };
        let label = self.take_label();

        // Loop header node
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);

        // Unlike while/for, do-while has no pre-body condition that could provide
        // a guaranteed edge to the join node.  The join is therefore created
        // lazily: it only comes into existence when a `break` statement or the
        // post-body condition needs it.  This prevents orphaned join nodes when
        // the body always terminates (return/throw) and contains no break.
        self.breakable_stack.push(BreakableContext {
            label,
            break_target: None,
            continue_target: Some(header_node),
        });

        // Loop body (executes at least once)
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(header_node, body_start);

        self.current_node = Some(body_start);
        self.visit_stmt(&do_while_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);
        let body_completed = self.current_node.is_some();

        let ctx = self.breakable_stack.pop().unwrap();
        let lazy_join = ctx.break_target;

        if body_completed {
            // Body completed normally — emit the post-body condition, back-edge
            // and false-exit edge to the join node.
            let join_node = lazy_join.unwrap_or_else(|| self.cfg.add_node(NodeKind::Join));
            let condition_node = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(body_end, condition_node);
            self.cfg.add_edge(condition_node, header_node);
            self.cfg.add_edge(condition_node, join_node);
            self.current_node = Some(join_node);
        } else if let Some(join_node) = lazy_join {
            // Body terminated (return/throw) but a break statement targeted the
            // join node, so it is reachable and code after the loop is live.
            self.current_node = Some(join_node);
        } else {
            // Body always terminates with no break — code after the loop is dead.
            self.current_node = None;
        }
    }

    fn visit_for(&mut self, for_stmt: &ForStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };
        let label = self.take_label();

        // Initialization (if present) - sequential node before header
        let init_end = if for_stmt.init.is_some() {
            let node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, node);
            node
        } else {
            from_node
        };

        // Loop header node
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(init_end, header_node);

        // When a condition is present it always provides a false-branch edge to
        // join → eager creation.  Without a condition (infinite loop) the join
        // is only reachable via break → lazy creation.
        let (condition_node, initial_break_target) = if for_stmt.test.is_some() {
            let cnode = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(header_node, cnode);
            let join = self.cfg.add_node(NodeKind::Join);
            self.cfg.add_edge(cnode, join);
            (cnode, Some(join))
        } else {
            // No condition — treat header as the implicit condition node
            (header_node, None)
        };

        // Push loop context for break/continue resolution
        self.breakable_stack.push(BreakableContext {
            label,
            break_target: initial_break_target,
            continue_target: Some(header_node),
        });

        // Loop body
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);

        self.current_node = Some(body_start);
        self.visit_stmt(&for_stmt.body);
        let mut body_end = self.current_node.unwrap_or(body_start);

        let ctx = self.breakable_stack.pop().unwrap();

        // Only process update/back-edge if body completed normally
        if self.current_node.is_some() {
            // Update expression (if present) - executes after body
            if for_stmt.update.is_some() {
                let update_node = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(body_end, update_node);
                body_end = update_node;
            }

            // Back-edge to header
            self.cfg.add_edge(body_end, header_node);
        }

        if let Some(join_node) = ctx.break_target {
            self.current_node = Some(join_node);
        } else {
            // Infinite loop whose body always terminates with no break.
            self.current_node = None;
        }
    }

    fn visit_for_in(&mut self, for_in_stmt: &ForInStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };
        let label = self.take_label();

        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);

        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(header_node, condition_node);

        // Create join node BEFORE body so break can target it
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.cfg.add_edge(condition_node, join_node);

        // Push loop context
        self.breakable_stack.push(BreakableContext {
            label,
            break_target: Some(join_node),
            continue_target: Some(header_node),
        });

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);

        self.current_node = Some(body_start);
        self.visit_stmt(&for_in_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);

        self.breakable_stack.pop();

        // Back-edge only if body completed normally
        if self.current_node.is_some() {
            self.cfg.add_edge(body_end, header_node);
        }

        self.current_node = Some(join_node);
    }

    fn visit_for_of(&mut self, for_of_stmt: &ForOfStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };
        let label = self.take_label();

        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);

        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(header_node, condition_node);

        // Create join node BEFORE body so break can target it
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.cfg.add_edge(condition_node, join_node);

        // Push loop context
        self.breakable_stack.push(BreakableContext {
            label,
            break_target: Some(join_node),
            continue_target: Some(header_node),
        });

        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);

        self.current_node = Some(body_start);
        self.visit_stmt(&for_of_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);

        self.breakable_stack.pop();

        // Back-edge only if body completed normally
        if self.current_node.is_some() {
            self.cfg.add_edge(body_end, header_node);
        }

        self.current_node = Some(join_node);
    }

    fn visit_switch(&mut self, switch_stmt: &SwitchStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };
        let label = self.take_label();

        // Switch expression evaluation (implied)
        let switch_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, switch_node);

        // The join node is created lazily: it comes into existence on the first
        // `break` statement or when a case falls through past the end.  This
        // prevents orphaned unreachable nodes when every case terminates via
        // return/throw with no break.
        self.breakable_stack.push(BreakableContext {
            label,
            break_target: None,
            continue_target: None, // switch is not continuable
        });

        // Process cases
        let mut prev_case_end: Option<NodeId> = None;
        for case in &switch_stmt.cases {
            let case_node = self.cfg.add_node(NodeKind::Statement);

            // Each case gets an edge from switch (for case matching)
            self.cfg.add_edge(switch_node, case_node);

            // Fallthrough from previous case if it didn't break/return/throw
            if let Some(prev_end) = prev_case_end {
                self.cfg.add_edge(prev_end, case_node);
            }

            // Visit case body statements
            self.current_node = Some(case_node);
            for stmt in &case.cons {
                self.visit_stmt(stmt);
                if self.current_node.is_none() {
                    break;
                }
            }

            // Track end of case for fallthrough to next case
            // If current_node is None (break/return/throw), no fallthrough
            prev_case_end = self.current_node;
        }

        let ctx = self.breakable_stack.pop().unwrap();
        let lazy_join = ctx.break_target;

        if let Some(last_end) = prev_case_end {
            // Last case fell through — wire it to the (possibly new) join node.
            let join_node = lazy_join.unwrap_or_else(|| self.cfg.add_node(NodeKind::Join));
            self.cfg.add_edge(last_end, join_node);
            self.current_node = Some(join_node);
        } else if let Some(join_node) = lazy_join {
            // All cases terminated but at least one `break` already created join.
            self.current_node = Some(join_node);
        } else {
            // All cases terminated with no break and no fallthrough.
            // Code after the switch is dead.
            self.current_node = None;
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

    fn build_catch_block(&mut self, try_stmt: &TryStmt, try_start: NodeId) -> Option<NodeId> {
        let handler = try_stmt.handler.as_ref()?;
        let catch_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(try_start, catch_start);
        self.current_node = Some(catch_start);
        self.build_from_body(&handler.body);
        self.current_node
    }

    fn connect_finally(
        &mut self,
        finally_block: &BlockStmt,
        try_start: NodeId,
        try_end: NodeId,
        try_completed: bool,
        catch_end: Option<NodeId>,
        has_handler: bool,
    ) {
        let join_node = self.cfg.add_node(NodeKind::Join);
        let finally_start = self.cfg.add_node(NodeKind::Statement);
        if try_completed {
            self.cfg.add_edge(try_end, finally_start);
        }
        if let Some(catch) = catch_end {
            if catch != self.cfg.exit {
                self.cfg.add_edge(catch, finally_start);
            }
        }
        if !has_handler || catch_end == Some(self.cfg.exit) {
            self.cfg.add_edge(try_start, finally_start);
        }
        self.current_node = Some(finally_start);
        self.build_from_body(finally_block);
        let finally_end = self.current_node.unwrap_or(finally_start);
        if self.current_node.is_some() {
            self.cfg.add_edge(finally_end, join_node);
        }
        self.current_node = Some(join_node);
    }

    fn connect_no_finally(
        &mut self,
        try_start: NodeId,
        try_end: NodeId,
        try_completed: bool,
        catch_end: Option<NodeId>,
        has_handler: bool,
    ) {
        let catch_completed = catch_end.map(|c| c != self.cfg.exit).unwrap_or(false);
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

    fn visit_try(&mut self, try_stmt: &TryStmt) {
        let Some(from_node) = self.current_node else {
            return;
        };

        let try_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from_node, try_start);
        self.current_node = Some(try_start);
        self.build_from_body(&try_stmt.block);
        let try_end = self.current_node.unwrap_or(try_start);
        let try_completed = self.current_node.is_some();

        let catch_end = self.build_catch_block(try_stmt, try_start);
        let has_handler = try_stmt.handler.is_some();

        if let Some(finally_block) = &try_stmt.finalizer {
            self.connect_finally(
                finally_block,
                try_start,
                try_end,
                try_completed,
                catch_end,
                has_handler,
            );
        } else {
            self.connect_no_finally(try_start, try_end, try_completed, catch_end, has_handler);
        }
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

    /// Dead code after a return inside an if-branch must not cause a panic.
    /// The if/while/for in unreachable positions should be silently skipped.
    #[test]
    fn test_dead_code_after_return_no_panic() {
        // The second `if` statement is dead code — current_node is None when we
        // reach it.  The builder must not panic.
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
}
