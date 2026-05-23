//! C# CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use crate::language::tree_sitter_utils::{
    find_child_by_kind, find_function_by_start, with_cached_csharp_tree,
};
use tree_sitter::Node;

pub struct CSharpCfgBuilder;

impl CfgBuilder for CSharpCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let (_body_node_id, source) = function.body.as_csharp();

        let result = with_cached_csharp_tree(source, |root| {
            let func_node = find_function_by_start(
                root,
                function.span.start,
                &[
                    "method_declaration",
                    "constructor_declaration",
                    "local_function_statement",
                    "operator_declaration",
                    "conversion_operator_declaration",
                ],
            )?;
            let body_node = find_child_by_kind(func_node, "block")?;
            let mut builder = CSharpCfgBuilderState::new();
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

struct CSharpCfgBuilderState {
    cfg: Cfg,
    current_node: Option<NodeId>,
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    break_target: NodeId,
    continue_target: NodeId,
}

impl CSharpCfgBuilderState {
    fn new() -> Self {
        let cfg = Cfg::new();
        let entry = cfg.entry;
        CSharpCfgBuilderState {
            cfg,
            current_node: Some(entry),
            loop_stack: Vec::new(),
        }
    }

    fn build_from_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();
        for child in block.children(&mut cursor) {
            if child.is_named() {
                self.visit_node(&child, source);
            }
        }

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

    fn visit_node(&mut self, node: &Node, source: &str) {
        match node.kind() {
            "if_statement" => self.visit_if(node, source),
            "while_statement" => self.visit_while(node, source),
            "do_statement" => self.visit_do_while(node, source),
            "for_statement" => self.visit_for(node, source),
            "foreach_statement" => self.visit_foreach(node, source),
            "switch_statement" => self.visit_switch(node, source),
            "try_statement" => self.visit_try(node, source),
            "return_statement" => self.visit_return(),
            "throw_statement" => self.visit_throw(),
            "break_statement" => self.visit_break(),
            "continue_statement" => self.visit_continue(),
            "local_function_statement" => {
                // Local functions are discovered separately; skip the body here
            }
            _ => self.visit_simple_statement(),
        }
    }

    fn visit_if(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        let condition = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, condition);

        let join = self.cfg.add_node(NodeKind::Statement);

        // Then branch: first "block" child
        let then_block = find_child_by_kind(*node, "block");
        if let Some(block) = then_block {
            self.current_node = Some(condition);
            self.visit_block(&block, source);
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, join);
                }
            }
        } else {
            self.cfg.add_edge(condition, join);
        }

        // Else branch: "else_clause" child
        let else_clause = find_child_by_kind(*node, "else_clause");
        if let Some(else_node) = else_clause {
            self.current_node = Some(condition);
            let mut cursor = else_node.walk();
            for child in else_node.children(&mut cursor) {
                if child.is_named() {
                    if child.kind() == "if_statement" {
                        self.visit_if(&child, source);
                    } else if child.kind() == "block" {
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
            self.cfg.add_edge(condition, join);
        }

        self.current_node = Some(join);
    }

    fn visit_while(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        let condition = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, condition);

        let after_loop = self.cfg.add_node(NodeKind::Statement);

        self.loop_stack.push(LoopContext {
            break_target: after_loop,
            continue_target: condition,
        });

        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(condition);
            self.visit_block(&body, source);
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, condition);
                }
            }
        }

        self.cfg.add_edge(condition, after_loop);
        self.loop_stack.pop();
        self.current_node = Some(after_loop);
    }

    fn visit_do_while(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        let body_entry = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(current, body_entry);

        let condition = self.cfg.add_node(NodeKind::Condition);
        let after_loop = self.cfg.add_node(NodeKind::Statement);

        self.loop_stack.push(LoopContext {
            break_target: after_loop,
            continue_target: condition,
        });

        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(body_entry);
            self.visit_block(&body, source);
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, condition);
                }
            }
        } else {
            self.cfg.add_edge(body_entry, condition);
        }

        self.cfg.add_edge(condition, body_entry);
        self.cfg.add_edge(condition, after_loop);
        self.loop_stack.pop();
        self.current_node = Some(after_loop);
    }

    fn visit_for(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        let condition = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, condition);

        let after_loop = self.cfg.add_node(NodeKind::Statement);

        self.loop_stack.push(LoopContext {
            break_target: after_loop,
            continue_target: condition,
        });

        if let Some(body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(condition);
            self.visit_block(&body, source);
            if let Some(last) = self.current_node {
                if last != self.cfg.exit {
                    self.cfg.add_edge(last, condition);
                }
            }
        }

        self.cfg.add_edge(condition, after_loop);
        self.loop_stack.pop();
        self.current_node = Some(after_loop);
    }

    fn visit_foreach(&mut self, node: &Node, source: &str) {
        // foreach is structurally identical to for from a CFG perspective
        self.visit_for(node, source);
    }

    fn visit_switch(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        let switch_node = self.cfg.add_node(NodeKind::Condition);
        self.cfg.add_edge(current, switch_node);

        let join = self.cfg.add_node(NodeKind::Statement);

        self.loop_stack.push(LoopContext {
            break_target: join,
            continue_target: join,
        });

        // C# switch_statement contains a switch_body with switch_section children
        if let Some(body) = find_child_by_kind(*node, "switch_body") {
            let mut cursor = body.walk();
            let mut has_default = false;

            for child in body.children(&mut cursor) {
                if child.kind() == "switch_section" {
                    let case_node = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(switch_node, case_node);

                    // Check for default label
                    let mut inner = child.walk();
                    for label in child.children(&mut inner) {
                        if label.kind() == "default_switch_label" {
                            has_default = true;
                        }
                    }

                    self.current_node = Some(case_node);
                    let mut stmt_cursor = child.walk();
                    for stmt in child.children(&mut stmt_cursor) {
                        if stmt.is_named()
                            && stmt.kind() != "case_switch_label"
                            && stmt.kind() != "default_switch_label"
                        {
                            self.visit_node(&stmt, source);
                        }
                    }

                    if let Some(last) = self.current_node {
                        if last != self.cfg.exit {
                            self.cfg.add_edge(last, join);
                        }
                    }
                }
            }

            if !has_default {
                self.cfg.add_edge(switch_node, join);
            }
        } else {
            self.cfg.add_edge(switch_node, join);
        }

        self.loop_stack.pop();
        self.current_node = Some(join);
    }

    fn visit_try(&mut self, node: &Node, source: &str) {
        let Some(current) = self.current_node else {
            return;
        };

        let try_entry = self.cfg.add_node(NodeKind::Statement);
        self.cfg.add_edge(current, try_entry);

        let mut branch_ends = Vec::new();

        if let Some(try_body) = find_child_by_kind(*node, "block") {
            self.current_node = Some(try_entry);
            self.visit_block(&try_body, source);
            if let Some(last) = self.current_node {
                branch_ends.push(last);
            }
        } else {
            branch_ends.push(try_entry);
        }

        // catch_clause children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "catch_clause" {
                let catch_node = self.cfg.add_node(NodeKind::Condition);
                self.cfg.add_edge(try_entry, catch_node);

                if let Some(catch_body) = find_child_by_kind(child, "block") {
                    self.current_node = Some(catch_node);
                    self.visit_block(&catch_body, source);
                    if let Some(last) = self.current_node {
                        branch_ends.push(last);
                    }
                } else {
                    branch_ends.push(catch_node);
                }
            }
        }

        let non_exit: Vec<_> = branch_ends
            .into_iter()
            .filter(|&end| end != self.cfg.exit)
            .collect();

        if !non_exit.is_empty() {
            let join = self.cfg.add_node(NodeKind::Statement);
            for end in non_exit {
                self.cfg.add_edge(end, join);
            }
            self.current_node = Some(join);
        } else {
            self.current_node = Some(self.cfg.exit);
        }
    }

    fn visit_return(&mut self) {
        if let Some(current) = self.current_node {
            self.cfg.add_edge(current, self.cfg.exit);
            self.current_node = None;
        }
    }

    fn visit_throw(&mut self) {
        if let Some(current) = self.current_node {
            self.cfg.add_edge(current, self.cfg.exit);
            self.current_node = None;
        }
    }

    fn visit_break(&mut self) {
        if let Some(current) = self.current_node {
            if let Some(ctx) = self.loop_stack.last() {
                self.cfg.add_edge(current, ctx.break_target);
            } else {
                self.cfg.add_edge(current, self.cfg.exit);
            }
            self.current_node = None;
        }
    }

    fn visit_continue(&mut self) {
        if let Some(current) = self.current_node {
            if let Some(ctx) = self.loop_stack.last() {
                self.cfg.add_edge(current, ctx.continue_target);
            } else {
                self.cfg.add_edge(current, self.cfg.exit);
            }
            self.current_node = None;
        }
    }

    fn visit_simple_statement(&mut self) {
        if let Some(current) = self.current_node {
            let node = self.cfg.add_node(NodeKind::Statement);
            self.cfg.add_edge(current, node);
            self.current_node = Some(node);
        }
    }

    fn visit_block(&mut self, block: &Node, source: &str) {
        let mut cursor = block.walk();
        for child in block.children(&mut cursor) {
            if child.is_named() {
                self.visit_node(&child, source);
            }
        }
    }
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
            body: FunctionBody::CSharp {
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
    public void Foo() {
        int x = 1;
    }
}
"#;
        let start = source.find("public void Foo").unwrap();
        let function = make_test_function(source, start, source.len());
        let cfg = CSharpCfgBuilder.build(&function);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 2);
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
public class Test {
    public void Foo(int x) {
        if (x > 0) {
            return;
        }
    }
}
"#;
        let start = source.find("public void Foo").unwrap();
        let function = make_test_function(source, start, source.len());
        let cfg = CSharpCfgBuilder.build(&function);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3);
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
public class Test {
    public void Foo(int n) {
        int i = 0;
        while (i < n) {
            i++;
        }
    }
}
"#;
        let start = source.find("public void Foo").unwrap();
        let function = make_test_function(source, start, source.len());
        let cfg = CSharpCfgBuilder.build(&function);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3);
    }

    #[test]
    fn test_foreach_loop() {
        let source = r#"
public class Test {
    public void Foo(int[] items) {
        foreach (var item in items) {
            Console.WriteLine(item);
        }
    }
}
"#;
        let start = source.find("public void Foo").unwrap();
        let function = make_test_function(source, start, source.len());
        let cfg = CSharpCfgBuilder.build(&function);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 3);
    }

    #[test]
    fn test_try_catch() {
        let source = r#"
public class Test {
    public void Foo(int x) {
        try {
            int result = 10 / x;
        } catch (DivideByZeroException e) {
            Console.WriteLine("error");
        }
    }
}
"#;
        let start = source.find("public void Foo").unwrap();
        let function = make_test_function(source, start, source.len());
        let cfg = CSharpCfgBuilder.build(&function);
        cfg.validate().unwrap();
        assert!(cfg.node_count() >= 4);
    }
}
