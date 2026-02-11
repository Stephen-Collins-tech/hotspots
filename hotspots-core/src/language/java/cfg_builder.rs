//! Java CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use tree_sitter::{Node, Parser};

/// Java CFG builder
///
/// Builds control flow graphs from Java function bodies parsed with tree-sitter.
pub struct JavaCfgBuilder;

impl CfgBuilder for JavaCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_java();

        // Re-parse the source to get the tree
        let mut parser = Parser::new();
        let language = tree_sitter_java::LANGUAGE;
        parser
            .set_language(&language.into())
            .expect("Failed to set Java language");

        let tree = parser
            .parse(source, None)
            .expect("Failed to re-parse Java source");
        let root = tree.root_node();

        // Find the function/method node in the tree
        if let Some(func_node) = find_function_by_start(root, function.span.start) {
            // Find the block (method body) or constructor_body
            if let Some(body_node) = find_child_by_kind(func_node, "block")
                .or_else(|| find_child_by_kind(func_node, "constructor_body"))
            {
                let mut builder = JavaCfgBuilderState::new();
                builder.build_from_block(&body_node, source);
                return builder.cfg;
            }
        }

        // Fallback: simple entry->exit CFG if we can't find the function
        let mut cfg = Cfg::new();
        cfg.add_edge(cfg.entry, cfg.exit);
        cfg
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
    fn build_from_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();

        for child in block.children(&mut cursor) {
            // Skip structural nodes (braces), process only named children
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
            "while_statement" => self.visit_while(node, source),
            "do_statement" => self.visit_do_while(node, source),
            "for_statement" | "enhanced_for_statement" => self.visit_for(node, source),
            "switch_statement" | "switch_expression" => self.visit_switch(node, source),
            "try_statement" => self.visit_try(node, source),
            "synchronized_statement" => self.visit_synchronized(node, source),
            "return_statement" => self.visit_return(),
            "throw_statement" => self.visit_throw(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "expression_statement" => self.visit_expression_statement(node, source),
            "local_variable_declaration" => self.visit_simple_statement(),
            "assert_statement" => self.visit_simple_statement(),
            _ => {
                // For other node types, just create a simple node
                self.visit_simple_statement();
            }
        }
    }

    /// Visit if statement
    fn visit_if(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create condition node
        let condition = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, condition);

        // Get the consequence block
        let consequence = find_child_by_kind(*node, "block");

        // Get alternative (else if or else)
        let alternative = find_child_by_kind(*node, "else_clause");

        // Join point after if
        let join = self.cfg.add_node(NodeKind::Statement);

        // Process consequence branch
        if let Some(cons_block) = consequence {
            self.current_node = Some(condition);
            self.visit_block(&cons_block, source);
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, join);
                }
            }
        } else {
            self.cfg.add_edge(condition, join);
        }

        // Process alternative branch (else or else if)
        if let Some(alt_node) = alternative {
            self.current_node = Some(condition);

            // Check if it's an else-if or plain else
            let mut cursor = alt_node.walk();
            for child in alt_node.children(&mut cursor) {
                if child.is_named() {
                    if child.kind() == "if_statement" {
                        // else if - visit as a nested if
                        self.visit_if(&child, source);
                    } else if child.kind() == "block" {
                        // else block
                        self.visit_block(&child, source);
                    }
                }
            }

            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, join);
                }
            }
        } else {
            // No else clause, condition can go directly to join
            self.cfg.add_edge(condition, join);
        }

        self.current_node = Some(join);
    }

    /// Visit while loop
    fn visit_while(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create condition node
        let condition = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, condition);

        // After loop join point
        let after_loop = self.cfg.add_node(NodeKind::Statement);

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_target: after_loop,
            continue_target: condition,
        });

        // Process body
        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(condition);
            self.visit_block(&body, source);

            // Loop back to condition
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, condition);
                }
            }
        }

        // Condition false edge to after loop
        self.cfg.add_edge(condition, after_loop);

        self.loop_stack.pop();
        self.current_node = Some(after_loop);
    }

    /// Visit do-while loop
    fn visit_do_while(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create body entry node
        let body_entry = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(current, body_entry);

        // Create condition node
        let condition = self.cfg.add_node(NodeKind::Condition);

        // After loop join point
        let after_loop = self.cfg.add_node(NodeKind::Statement);

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_target: after_loop,
            continue_target: condition,
        });

        // Process body
        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(body_entry);
            self.visit_block(&body, source);

            // Body goes to condition
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, condition);
                }
            }
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

    /// Visit for loop (traditional or enhanced)
    fn visit_for(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create condition node (header)
        let condition = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, condition);

        // After loop join point
        let after_loop = self.cfg.add_node(NodeKind::Statement);

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_target: after_loop,
            continue_target: condition,
        });

        // Process body
        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(condition);
            self.visit_block(&body, source);

            // Loop back to condition
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, condition);
                }
            }
        }

        // Condition false edge to after loop
        self.cfg.add_edge(condition, after_loop);

        self.loop_stack.pop();
        self.current_node = Some(after_loop);
    }

    /// Visit switch statement or switch expression
    fn visit_switch(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create switch node (decision point)
        let switch_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, switch_node);

        // Join point after switch
        let join = self.cfg.add_node(NodeKind::Statement);

        // Push loop context for break (switch can be broken out of)
        self.loop_stack.push(LoopContext {
            break_target: join,
            continue_target: join, // continue not valid in switch, but use join
        });

        // Find switch body
        if let Some(switch_body) = find_child_by_kind(*node, "switch_block") {
            let mut cursor = switch_body.walk();
            let mut has_default = false;

            for child in switch_body.children(&mut cursor) {
                if child.kind() == "switch_label" {
                    // Create a node for this case
                    let case_node = self.cfg.add_node(NodeKind::Statement);

                    // Switch branches to this case
                    self.cfg.add_edge(switch_node, case_node);

                    // Check if it's default case
                    let case_text = &source[child.start_byte()..child.end_byte()];
                    if case_text.contains("default") {
                        has_default = true;
                    }

                    self.current_node = Some(case_node);
                } else if child.is_named() && child.kind() != "{" && child.kind() != "}" {
                    // Process statements in the case
                    self.visit_node(&child, source);
                }
            }

            // Connect last case to join (fall-through or explicit break)
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, join);
                }
            }

            // If no default case, switch can go directly to join
            if !has_default {
                self.cfg.add_edge(switch_node, join);
            }
        } else {
            // No body, just go to join
            self.cfg.add_edge(switch_node, join);
        }

        self.loop_stack.pop();
        self.current_node = Some(join);
    }

    /// Visit try statement
    fn visit_try(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create try block entry
        let try_entry = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(current, try_entry);

        // Track all branch ends
        let mut branch_ends = Vec::new();

        // Process try body
        if let Some(try_body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(try_entry);
            self.visit_block(&try_body, source);

            if let Some(last) = self.current_node {
                branch_ends.push(last);
            }
        } else {
            // No try body
            branch_ends.push(try_entry);
        }

        // Process catch clauses
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "catch_clause" {
                // Each catch is a separate branch from try entry
                let catch_node = self.cfg.add_node(NodeKind::Condition);
                self.cfg.add_edge(try_entry, catch_node);

                if let Some(catch_body) = find_child_by_kind(child, "block") {
                    self.current_node = Some(catch_node);
                    self.visit_block(&catch_body, source);

                    if let Some(last) = self.current_node {
                        branch_ends.push(last);
                    }
                } else {
                    // No catch body
                    branch_ends.push(catch_node);
                }
            }
        }

        // Process finally block if present
        // Note: Finally blocks are complex to model correctly in CFG because they execute
        // on all exit paths. For simplicity, we skip modeling finally in the CFG.

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

    /// Visit synchronized statement
    fn visit_synchronized(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        // Create synchronized node (decision point - acquiring lock)
        let sync_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, sync_node);

        // Process body
        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(sync_node);
            self.visit_block(&body, source);
        } else {
            self.current_node = Some(sync_node);
        }
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

    /// Visit expression statement (may contain ternary, &&, ||, lambdas)
    fn visit_expression_statement(&mut self, _node: &Node, _source: &str) {
        // For now, treat as simple statement
        // Future enhancement: detect ternary, boolean operators, lambdas
        self.visit_simple_statement();

        // TODO: Check for conditional_expression (ternary)
        // TODO: Check for binary_expression with && or ||
        // TODO: Check for lambda_expression with control flow
    }

    /// Visit simple statement (no control flow impact)
    fn visit_simple_statement(&mut self) {
        if let Some(current) = self.current_node {
            let node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(current, node);
            self.current_node = Some(node);
        }
    }

    /// Visit a block node
    fn visit_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();
        for child in block.children(&mut cursor) {
            if child.is_named() {
                self.visit_node(&child, source);
            }
        }
    }
}

/// Find a function/method node by its start byte position
fn find_function_by_start(node: Node, start_byte: usize) -> Option<Node> {
    if (node.kind() == "method_declaration" || node.kind() == "constructor_declaration")
        && node.start_byte() == start_byte
    {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_by_start(child, start_byte) {
            return Some(found);
        }
    }

    None
}

/// Find a child node by kind
#[allow(clippy::manual_find)]
fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
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
                source: source.to_string(),
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
}
