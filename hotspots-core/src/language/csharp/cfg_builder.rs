//! C# CFG builder implementation
//!
//! Walks the tree-sitter-c-sharp AST iteratively via an explicit
//! heap-allocated work stack (`Task`, driven by the `run` loop, with results
//! threaded through `self.current_node`) instead of native function-call
//! recursion, so nesting depth is bounded by heap memory rather than the
//! thread stack (https://github.com/Stephen-Collins-tech/hotspots/issues/160).

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
            builder.build_from_block(body_node);
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

/// Pending unit of work on the explicit stack.
///
/// The start variants (`If`, `Loop`, `Switch`, ...) do what the old recursive
/// `visit_*` methods did on the way down; the `*Finish`/`*After*` variants do
/// what they did with the nested result on the way back up. The "result" of
/// nested work is read from `self.current_node`, exactly as the recursive
/// methods used it.
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
    /// Shared by `while`, `for` and `foreach`.
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
    SectionStart {
        switch_node: NodeId,
    },
    SectionFinish {
        join: NodeId,
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

    fn build_from_block(&mut self, block: Node<'_>) {
        let mut stack = vec![Task::FromBlock { block }];
        self.run(&mut stack);
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
    fn run<'a>(&mut self, stack: &mut Vec<Task<'a>>) {
        while let Some(task) = stack.pop() {
            match task {
                Task::FromBlock { block } => {
                    stack.push(Task::AfterFromBlock);
                    stack.push(Task::Block { block });
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
                Task::Block { block } => {
                    for child in named_children(&block).into_iter().rev() {
                        stack.push(Task::VisitNode { node: child });
                    }
                }
                Task::VisitNode { node } => self.dispatch_node(node, stack),
                Task::If { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let condition = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, condition);

                    let join = self.cfg.add_node(NodeKind::Statement);

                    // Then branch: first "block" child
                    let then_block = find_child_by_kind(node, "block");
                    stack.push(Task::IfAfterConsequence {
                        alternative: find_child_by_kind(node, "else_clause"),
                        condition,
                        join,
                        had_consequence: then_block.is_some(),
                    });
                    if let Some(block) = then_block {
                        self.current_node = Some(condition);
                        stack.push(Task::Block { block });
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

                    // Else branch: "else_clause" child
                    if let Some(else_node) = alternative {
                        self.current_node = Some(condition);
                        stack.push(Task::IfFinish { join });
                        for child in named_children(&else_node).into_iter().rev() {
                            match child.kind() {
                                "if_statement" => stack.push(Task::If { node: child }),
                                "block" => stack.push(Task::Block { block: child }),
                                _ => {}
                            }
                        }
                    } else {
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
                        self.connect_current_to(condition);
                    }

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
                        self.connect_current_to(condition);
                    } else {
                        self.cfg.add_edge(body_entry, condition);
                    }

                    self.cfg.add_edge(condition, body_entry);
                    self.cfg.add_edge(condition, after_loop);
                    self.loop_stack.pop();
                    self.current_node = Some(after_loop);
                }
                Task::Switch { node } => {
                    let Some(current) = self.current_node else {
                        continue;
                    };

                    let switch_node = self.cfg.add_node(NodeKind::Condition);
                    self.cfg.add_edge(current, switch_node);

                    let join = self.cfg.add_node(NodeKind::Statement);

                    self.loop_stack.push(LoopContext {
                        break_target: join,
                        continue_target: join,
                    });

                    // C# switch_statement contains a switch_body with switch_section children
                    let body = find_child_by_kind(node, "switch_body");
                    let sections: Vec<Node<'a>> = body
                        .map(|b| children_of_kind(&b, "switch_section"))
                        .unwrap_or_default();
                    let has_default = sections.iter().any(|section| {
                        let mut cursor = section.walk();
                        let found = section
                            .children(&mut cursor)
                            .any(|label| label.kind() == "default_switch_label");
                        found
                    });

                    stack.push(Task::SwitchFinish {
                        switch_node,
                        join,
                        has_default,
                        had_body: body.is_some(),
                    });
                    for section in sections.into_iter().rev() {
                        stack.push(Task::SectionFinish { join });
                        for stmt in named_children(&section).into_iter().rev() {
                            if stmt.kind() != "case_switch_label"
                                && stmt.kind() != "default_switch_label"
                            {
                                stack.push(Task::VisitNode { node: stmt });
                            }
                        }
                        stack.push(Task::SectionStart { switch_node });
                    }
                }
                Task::SectionStart { switch_node } => {
                    let case_node = self.cfg.add_node(NodeKind::Statement);
                    self.cfg.add_edge(switch_node, case_node);
                    self.current_node = Some(case_node);
                }
                Task::SectionFinish { join } => self.connect_current_to(join),
                Task::SwitchFinish {
                    switch_node,
                    join,
                    has_default,
                    had_body,
                } => {
                    if !had_body || !has_default {
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

                    let try_body = find_child_by_kind(node, "block");
                    // No try body: the entry node itself is the branch end
                    let branch_ends = if try_body.is_some() {
                        Vec::new()
                    } else {
                        vec![try_entry]
                    };
                    stack.push(Task::TryNext {
                        try_entry,
                        catches: children_of_kind(&node, "catch_clause"),
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
                }
            }
        }
    }

    fn dispatch_node<'a>(&mut self, node: Node<'a>, stack: &mut Vec<Task<'a>>) {
        match node.kind() {
            "if_statement" => stack.push(Task::If { node }),
            "while_statement" | "for_statement" | "foreach_statement" => {
                stack.push(Task::Loop { node })
            }
            "do_statement" => stack.push(Task::DoWhile { node }),
            "switch_statement" => stack.push(Task::Switch { node }),
            "try_statement" => stack.push(Task::Try { node }),
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
}

fn named_children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.is_named())
        .collect()
}

fn children_of_kind<'a>(node: &Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() == kind)
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
            body: FunctionBody::CSharp {
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

    /// Nesting this deep would overflow a recursive builder on a small stack.
    #[test]
    fn test_deeply_nested_if_else_does_not_overflow_and_builds_real_cfg() {
        const DEPTH: usize = 2_000;

        let mut source = String::from("class T {\nvoid Test(int x) {\n");
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
                use crate::language::{CSharpParser, LanguageParser};
                let parser = CSharpParser::new().unwrap();
                let module = parser.parse(&source, "T.cs").unwrap();
                let functions = module.discover_functions(0, &source);
                assert_eq!(functions.len(), 1);
                CSharpCfgBuilder.build(&functions[0]).node_count()
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
