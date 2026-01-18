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
    builder.build_from_body(&function.body);
    builder.cfg
}

/// Builder for constructing CFG from AST
struct CfgBuilder {
    cfg: Cfg,
    current_node: Option<NodeId>,
}

impl CfgBuilder {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;
        
        CfgBuilder {
            cfg,
            current_node: Some(entry),
        }
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
                let has_exit_edge = self.cfg.edges.iter()
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
        let from_node = self.current_node.expect("Current node should exist");
        
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
            if then_end != self.cfg.exit || (else_end != self.cfg.exit && self.current_node.is_some()) {
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
        let from_node = self.current_node.expect("Current node should exist");
        
        // Loop header node
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);
        
        // Condition node
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(header_node, condition_node);
        
        // Loop body
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);
        
        self.current_node = Some(body_start);
        self.visit_stmt(&while_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);
        
        // Back-edge to header (if body didn't terminate with return/throw)
        if self.current_node.is_some() {
            self.cfg.add_edge(body_end, header_node);
        }
        
        // Exit edge to join
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.cfg.add_edge(condition_node, join_node);
        self.current_node = Some(join_node);
    }

    fn visit_do_while(&mut self, do_while_stmt: &DoWhileStmt) {
        let from_node = self.current_node.expect("Current node should exist");
        
        // Loop header node
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);
        
        // Loop body (executes at least once)
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(header_node, body_start);
        
        self.current_node = Some(body_start);
        self.visit_stmt(&do_while_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);
        
        // Condition node (checked after body)
        // Only if body completed normally (didn't return/throw)
        let condition_node = if self.current_node.is_some() {
            let node = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(body_end, node);
            node
        } else {
            // Body terminated early - no condition check needed
            self.cfg.exit
        };
        
        // Back-edge to header if condition true (only if condition node was created)
        if condition_node != self.cfg.exit {
            self.cfg.add_edge(condition_node, header_node);
            
            // Exit edge to join if condition false
            let join_node = self.cfg.add_node(NodeKind::Join);
            self.cfg.add_edge(condition_node, join_node);
            self.current_node = Some(join_node);
        }
        // If body terminated early, current_node is already None, which is correct
    }

    fn visit_for(&mut self, for_stmt: &ForStmt) {
        let from_node = self.current_node.expect("Current node should exist");
        
        // Initialization (if present) - sequential node before header
        let init_node = if let Some(_init) = &for_stmt.init {
            let node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(from_node, node);
            Some(node)
        } else {
            Some(from_node)
        };
        
        // Loop header node
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        if let Some(init) = init_node {
            self.cfg.add_edge(init, header_node);
        }
        
        // Condition node (if present)
        let condition_node = if let Some(_cond) = &for_stmt.test {
            let node = self.cfg.add_node(NodeKind::Condition);
            self.cfg.add_edge(header_node, node);
            node
        } else {
            // No condition means infinite loop - condition always true
            header_node
        };
        
        // Loop body
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);
        
        self.current_node = Some(body_start);
        self.visit_stmt(&for_stmt.body);
        let mut body_end = self.current_node.unwrap_or(body_start);
        
        // Only process update/back-edge if body completed normally
        if self.current_node.is_some() {
            // Update expression (if present) - executes after body
            if let Some(_update) = &for_stmt.update {
                let update_node = self.cfg.add_node(NodeKind::Statement);
                self.cfg.add_edge(body_end, update_node);
                body_end = update_node;
            }
            
            // Back-edge to header
            self.cfg.add_edge(body_end, header_node);
        }
        
        // Exit edge to join
        let join_node = self.cfg.add_node(NodeKind::Join);
        if condition_node != header_node {
            self.cfg.add_edge(condition_node, join_node);
        }
        // Only set current_node to join if loop body completed normally
        if self.current_node.is_some() {
            self.current_node = Some(join_node);
        }
    }

    fn visit_for_in(&mut self, for_in_stmt: &ForInStmt) {
        // Similar to for loop but with for-in syntax
        let from_node = self.current_node.expect("Current node should exist");
        
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);
        
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(header_node, condition_node);
        
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);
        
        self.current_node = Some(body_start);
        self.visit_stmt(&for_in_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);
        
        // Back-edge only if body completed normally
        if self.current_node.is_some() {
            self.cfg.add_edge(body_end, header_node);
        }
        
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.cfg.add_edge(condition_node, join_node);
        // Only set current_node to join if loop body completed normally
        if self.current_node.is_some() {
            self.current_node = Some(join_node);
        }
    }

    fn visit_for_of(&mut self, for_of_stmt: &ForOfStmt) {
        // Similar to for-in
        let from_node = self.current_node.expect("Current node should exist");
        
        let header_node = self.cfg.add_node(NodeKind::LoopHeader);
        self.cfg.add_edge(from_node, header_node);
        
        let condition_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(header_node, condition_node);
        
        let body_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(condition_node, body_start);
        
        self.current_node = Some(body_start);
        self.visit_stmt(&for_of_stmt.body);
        let body_end = self.current_node.unwrap_or(body_start);
        
        // Back-edge only if body completed normally
        if self.current_node.is_some() {
            self.cfg.add_edge(body_end, header_node);
        }
        
        let join_node = self.cfg.add_node(NodeKind::Join);
        self.cfg.add_edge(condition_node, join_node);
        // Only set current_node to join if loop body completed normally
        if self.current_node.is_some() {
            self.current_node = Some(join_node);
        }
    }

    fn visit_switch(&mut self, switch_stmt: &SwitchStmt) {
        let from_node = self.current_node.expect("Current node should exist");
        
        // Switch expression evaluation (implied)
        let switch_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(from_node, switch_node);
        
        let join_node = self.cfg.add_node(NodeKind::Join);
        
        // Process cases
        let mut prev_case_node: Option<NodeId> = None;
        for case in &switch_stmt.cases {
            let case_node = self.cfg.add_node(NodeKind::Statement);
            
            // First case connects from switch, others from previous (fallthrough)
            if let Some(prev) = prev_case_node {
                self.cfg.add_edge(prev, case_node);
            } else {
                self.cfg.add_edge(switch_node, case_node);
            }
            
            // Visit case body statements
            self.current_node = Some(case_node);
            for stmt in &case.cons {
                self.visit_stmt(stmt);
                // If visit_stmt set current_node to None (return/throw), 
                // we need to handle it, but continue processing for other cases
                if self.current_node.is_none() {
                    // Statement terminated (return/throw) - restore case_node for next case
                    // But mark this case as having an exit
                    break;
                }
            }
            let case_end = self.current_node.unwrap_or(case_node);
            
            // Check if case ends with break (explicit or implicit via return/throw)
            let has_break = self.has_break_in_stmt(&case.cons);
            
            // If no break, case falls through to next case
            // Edge to next case already added above when we set up the chain
            // If break present, case exits to join
            if has_break {
                self.cfg.add_edge(case_end, join_node);
            }
            // If no break and not last case, fallthrough edge already exists
            
            prev_case_node = Some(case_node);
        }
        
        // Last case (if no break) also flows to join
        if let Some(last_case) = switch_stmt.cases.last() {
            let has_break = self.has_break_in_stmt(&last_case.cons);
            if !has_break {
                if let Some(last_end) = self.current_node {
                    self.cfg.add_edge(last_end, join_node);
                }
            }
        }
        
        self.current_node = Some(join_node);
    }

    fn visit_return(&mut self, _return_stmt: &ReturnStmt) {
        let from_node = self.current_node.expect("Current node should exist");
        
        // Return statement - edge directly to exit
        self.cfg.add_edge(from_node, self.cfg.exit);
        
        // No further execution after return
        self.current_node = None;
    }

    fn visit_throw(&mut self, _throw_stmt: &ThrowStmt) {
        let from_node = self.current_node.expect("Current node should exist");
        
        // Throw statement - edge directly to exit
        self.cfg.add_edge(from_node, self.cfg.exit);
        
        // No further execution after throw
        self.current_node = None;
    }

    fn visit_try(&mut self, try_stmt: &TryStmt) {
        let from_node = self.current_node.expect("Current node should exist");
        
        // Try body
        let try_start = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(from_node, try_start);
        
        self.current_node = Some(try_start);
        self.build_from_body(&try_stmt.block);
        let try_end = self.current_node.unwrap_or(try_start);
        
        // Catch block (if present - only one in JavaScript/TypeScript)
        let mut catch_end: Option<NodeId> = None;
        if let Some(handler) = &try_stmt.handler {
            let catch_start = self.cfg.add_node(NodeKind::Statement);
            // Try body can flow to catch (on exception)
            self.cfg.add_edge(try_start, catch_start);
            
            self.current_node = Some(catch_start);
            self.build_from_body(&handler.body);
            catch_end = self.current_node;
        }
        
        // If try ended with return/throw, we need to handle finally separately
        let try_completed = self.current_node.is_some();
        
        // Finally block (always executes if present)
        let join_node = self.cfg.add_node(NodeKind::Join);
        
        if let Some(finally_block) = &try_stmt.finalizer {
            let finally_start = self.cfg.add_node(NodeKind::Statement);
            // Try normal completion flows to finally (if completed normally)
            if try_completed {
                self.cfg.add_edge(try_end, finally_start);
            }
            // Catch completion also flows to finally
            if let Some(catch) = catch_end {
                if catch != self.cfg.exit {
                    self.cfg.add_edge(catch, finally_start);
                }
            }
            // Exception path (if no catch or catch didn't return)
            if try_stmt.handler.is_none() || catch_end == Some(self.cfg.exit) {
                self.cfg.add_edge(try_start, finally_start);
            }
            
            self.current_node = Some(finally_start);
            self.build_from_body(finally_block);
            let finally_end = self.current_node.unwrap_or(finally_start);
            
            // Finally flows to join
            if self.current_node.is_some() {
                self.cfg.add_edge(finally_end, join_node);
            }
            self.current_node = Some(join_node);
        } else {
            // No finally - join after try/catch
            if try_completed {
                self.cfg.add_edge(try_end, join_node);
            }
            if let Some(catch) = catch_end {
                if catch != self.cfg.exit {
                    self.cfg.add_edge(catch, join_node);
                }
            }
            // Exception path without catch goes to exit
            if try_stmt.handler.is_none() {
                self.cfg.add_edge(try_start, self.cfg.exit);
            }
            self.current_node = Some(join_node);
        }
    }

    fn visit_break(&mut self, _break_stmt: &BreakStmt) {
        // Break statements are handled by loop/switch context
        // For now, mark as non-structured exit
        // TODO: Track loop context to route to correct exit
        let from_node = self.current_node.expect("Current node should exist");
        
        // Placeholder: break goes to exit (will be refined with loop tracking)
        self.cfg.add_edge(from_node, self.cfg.exit);
        self.current_node = None;
    }

    fn visit_continue(&mut self, _continue_stmt: &ContinueStmt) {
        // Continue statements are handled by loop context
        // TODO: Track loop context to route to correct header
        let from_node = self.current_node.expect("Current node should exist");
        
        // Placeholder: continue goes to exit (will be refined with loop tracking)
        self.cfg.add_edge(from_node, self.cfg.exit);
        self.current_node = None;
    }

    /// Check if statements contain a break (or return/throw which also exits)
    fn has_break_in_stmt(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match stmt {
                Stmt::Break(_) | Stmt::Return(_) | Stmt::Throw(_) => return true,
                Stmt::If(if_stmt) => {
                    if self.has_break_in_stmt(&[if_stmt.cons.as_ref().clone()]) {
                        return true;
                    }
                    if let Some(alt) = &if_stmt.alt {
                        if self.has_break_in_stmt(&[alt.as_ref().clone()]) {
                            return true;
                        }
                    }
                }
                Stmt::Block(block_stmt) => {
                    if self.has_break_in_stmt(&block_stmt.stmts) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
}

impl Default for CfgBuilder {
    fn default() -> Self {
        Self::new()
    }
}
