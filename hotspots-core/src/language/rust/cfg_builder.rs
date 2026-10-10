//! Rust CFG builder implementation
//!
//! Walks the `syn` AST iteratively via an explicit heap-allocated work stack
//! (`Task`/results threaded through `last`) instead of native function-call
//! recursion. This removes the depth bound entirely — depth is now limited by
//! available heap/memory, not by the calling thread's stack — closing the
//! structural gap tracked in
//! https://github.com/Stephen-Collins-tech/hotspots/issues/160. (PR #161's
//! `MAX_CFG_DEPTH` guard, the mitigation this supersedes, only bounded the
//! recursive-descent *builder*; it did not change depth bounding for
//! `syn::parse_str` itself, which is a separate, still-open gap noted there.)

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use anyhow::{Context, Result};
use syn::{Arm, Block, Expr, ExprBlock, ExprForLoop, ExprIf, ExprLoop, ExprMatch, ExprWhile, Stmt};

/// CFG builder for Rust functions
pub struct RustCfgBuilder;

impl CfgBuilder for RustCfgBuilder {
    fn build(&self, function: &FunctionNode) -> Cfg {
        let source = function.body.as_rust();

        // Parse the function source
        // On error, return a minimal CFG (entry -> exit)
        build_cfg_from_source(source).unwrap_or_default()
    }
}

/// Build CFG from Rust source
fn build_cfg_from_source(source: &str) -> Result<Cfg> {
    // Parse the function source
    let item_fn: syn::ItemFn =
        syn::parse_str(source).context("Failed to parse Rust function for CFG building")?;

    let mut cfg = Cfg::new();
    let entry = cfg.entry;
    let exit = cfg.exit;

    // Build CFG from function block
    let last_node = build_block_cfg(&mut cfg, &item_fn.block, entry, exit);

    // Connect last node to exit
    cfg.add_edge(last_node, exit);

    Ok(cfg)
}

/// Pending unit of work on the explicit stack.
///
/// The "visit" variants mirror what the old recursive functions did on the
/// way down the call stack; the "after" variants mirror what they did with
/// the recursive result on the way back up, now driven by a loop instead of
/// stack unwinding. Each variant borrows from the same `'a` AST tree rooted
/// at the function body, which outlives the whole build.
enum Task<'a> {
    Block {
        stmts: &'a [Stmt],
        idx: usize,
        entry: NodeId,
        exit: NodeId,
    },
    ContinueBlock {
        stmts: &'a [Stmt],
        idx: usize,
        exit: NodeId,
    },
    Stmt {
        stmt: &'a Stmt,
        entry: NodeId,
        exit: NodeId,
    },
    Expr {
        expr: &'a Expr,
        entry: NodeId,
        exit: NodeId,
    },
    If {
        expr_if: &'a ExprIf,
        entry: NodeId,
        exit: NodeId,
    },
    AfterIfThen {
        join: NodeId,
        condition: NodeId,
        else_expr: Option<&'a Expr>,
        exit: NodeId,
    },
    AfterIfElse {
        join: NodeId,
    },
    Match {
        expr_match: &'a ExprMatch,
        entry: NodeId,
        exit: NodeId,
    },
    AfterMatchArm {
        arms: std::slice::Iter<'a, Arm>,
        condition: NodeId,
        join: NodeId,
        exit: NodeId,
    },
    Loop {
        expr_loop: &'a ExprLoop,
        entry: NodeId,
    },
    AfterLoopBody {
        header: NodeId,
    },
    While {
        expr_while: &'a ExprWhile,
        entry: NodeId,
    },
    AfterWhileBody {
        condition: NodeId,
    },
    For {
        expr_for: &'a ExprForLoop,
        entry: NodeId,
    },
    AfterForBody {
        condition: NodeId,
    },
    ExprBlock {
        expr_block: &'a ExprBlock,
        entry: NodeId,
        exit: NodeId,
    },
}

/// Build CFG for a block, driving the traversal from an explicit work stack
/// instead of native recursion so nesting depth is bounded only by heap size.
fn build_block_cfg(cfg: &mut Cfg, block: &Block, entry: NodeId, exit: NodeId) -> NodeId {
    let mut stack: Vec<Task<'_>> = vec![Task::Block {
        stmts: &block.stmts,
        idx: 0,
        entry,
        exit,
    }];
    // Running result of the most recently completed task; each "after"
    // task below consumes it as the output of the work it was waiting on.
    let mut last = entry;

    while let Some(task) = stack.pop() {
        match task {
            Task::Block {
                stmts,
                idx,
                entry,
                exit,
            } => {
                if idx >= stmts.len() {
                    last = entry;
                } else {
                    stack.push(Task::ContinueBlock {
                        stmts,
                        idx: idx + 1,
                        exit,
                    });
                    stack.push(Task::Stmt {
                        stmt: &stmts[idx],
                        entry,
                        exit,
                    });
                }
            }
            Task::ContinueBlock { stmts, idx, exit } => {
                let entry = last;
                if idx >= stmts.len() {
                    last = entry;
                } else {
                    stack.push(Task::ContinueBlock {
                        stmts,
                        idx: idx + 1,
                        exit,
                    });
                    stack.push(Task::Stmt {
                        stmt: &stmts[idx],
                        entry,
                        exit,
                    });
                }
            }
            Task::Stmt { stmt, entry, exit } => match stmt {
                Stmt::Expr(expr, _) => stack.push(Task::Expr { expr, entry, exit }),
                Stmt::Local(local) => {
                    // Variable declaration — walk the initializer expression so
                    // if/if-let used as a let-initializer still builds its branch CFG.
                    if let Some(init) = &local.init {
                        stack.push(Task::Expr {
                            expr: &init.expr,
                            entry,
                            exit,
                        });
                    } else {
                        let node = cfg.add_node(NodeKind::Statement);
                        cfg.add_edge(entry, node);
                        last = node;
                    }
                }
                Stmt::Item(_) | Stmt::Macro(_) => {
                    // Nested item or macro invocation — treat as statement
                    let node = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(entry, node);
                    last = node;
                }
            },
            Task::Expr { expr, entry, exit } => match expr {
                Expr::If(expr_if) => stack.push(Task::If {
                    expr_if,
                    entry,
                    exit,
                }),
                Expr::Match(expr_match) => stack.push(Task::Match {
                    expr_match,
                    entry,
                    exit,
                }),
                Expr::Loop(expr_loop) => stack.push(Task::Loop { expr_loop, entry }),
                Expr::While(expr_while) => stack.push(Task::While { expr_while, entry }),
                Expr::ForLoop(expr_for) => stack.push(Task::For { expr_for, entry }),
                Expr::Block(expr_block) => stack.push(Task::ExprBlock {
                    expr_block,
                    entry,
                    exit,
                }),
                Expr::Return(_) => {
                    // Return statement - connects to exit
                    let node = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(entry, node);
                    cfg.add_edge(node, exit);
                    last = node;
                }
                Expr::Break(_) => {
                    // Break statement
                    // Note: In a full implementation, we'd route break to loop exit
                    let node = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(entry, node);
                    last = node;
                }
                Expr::Continue(_) => {
                    // Continue statement
                    // Note: In a full implementation, we'd route continue to loop header
                    let node = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(entry, node);
                    last = node;
                }
                _ => {
                    // Other expressions (calls, literals, etc.)
                    let node = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(entry, node);
                    last = node;
                }
            },
            Task::If {
                expr_if,
                entry,
                exit,
            } => {
                let condition = cfg.add_node(NodeKind::Condition);
                cfg.add_edge(entry, condition);

                let then_entry = cfg.add_node(NodeKind::Statement);
                cfg.add_edge(condition, then_entry);
                let join = cfg.add_node(NodeKind::Join);

                stack.push(Task::AfterIfThen {
                    join,
                    condition,
                    else_expr: expr_if.else_branch.as_ref().map(|(_, e)| e.as_ref()),
                    exit,
                });
                stack.push(Task::Block {
                    stmts: &expr_if.then_branch.stmts,
                    idx: 0,
                    entry: then_entry,
                    exit,
                });
            }
            Task::AfterIfThen {
                join,
                condition,
                else_expr,
                exit,
            } => {
                let then_exit = last;
                cfg.add_edge(then_exit, join);

                match else_expr {
                    Some(else_expr) => {
                        let else_entry = cfg.add_node(NodeKind::Statement);
                        cfg.add_edge(condition, else_entry);
                        stack.push(Task::AfterIfElse { join });
                        stack.push(Task::Expr {
                            expr: else_expr,
                            entry: else_entry,
                            exit,
                        });
                    }
                    None => {
                        // No else branch - condition can go directly to join
                        cfg.add_edge(condition, join);
                        last = join;
                    }
                }
            }
            Task::AfterIfElse { join } => {
                let else_exit = last;
                cfg.add_edge(else_exit, join);
                last = join;
            }
            Task::Match {
                expr_match,
                entry,
                exit,
            } => {
                let condition = cfg.add_node(NodeKind::Condition);
                cfg.add_edge(entry, condition);
                let join = cfg.add_node(NodeKind::Join);

                let mut arms = expr_match.arms.iter();
                if let Some(first_arm) = arms.next() {
                    let arm_entry = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(condition, arm_entry);
                    stack.push(Task::AfterMatchArm {
                        arms,
                        condition,
                        join,
                        exit,
                    });
                    stack.push(Task::Expr {
                        expr: &first_arm.body,
                        entry: arm_entry,
                        exit,
                    });
                } else {
                    cfg.add_edge(condition, join);
                    last = join;
                }
            }
            Task::AfterMatchArm {
                mut arms,
                condition,
                join,
                exit,
            } => {
                let arm_exit = last;
                cfg.add_edge(arm_exit, join);

                if let Some(next_arm) = arms.next() {
                    let arm_entry = cfg.add_node(NodeKind::Statement);
                    cfg.add_edge(condition, arm_entry);
                    stack.push(Task::AfterMatchArm {
                        arms,
                        condition,
                        join,
                        exit,
                    });
                    stack.push(Task::Expr {
                        expr: &next_arm.body,
                        entry: arm_entry,
                        exit,
                    });
                } else {
                    last = join;
                }
            }
            Task::Loop { expr_loop, entry } => {
                let header = cfg.add_node(NodeKind::LoopHeader);
                cfg.add_edge(entry, header);

                stack.push(Task::AfterLoopBody { header });
                stack.push(Task::Block {
                    stmts: &expr_loop.body.stmts,
                    idx: 0,
                    entry: header,
                    exit: header,
                });
            }
            Task::AfterLoopBody { header } => {
                let body_exit = last;
                // Back edge to header
                cfg.add_edge(body_exit, header);

                // Loop exit (for breaks)
                let loop_exit = cfg.add_node(NodeKind::Join);
                cfg.add_edge(header, loop_exit);
                last = loop_exit;
            }
            Task::While { expr_while, entry } => {
                let condition = cfg.add_node(NodeKind::Condition);
                cfg.add_edge(entry, condition);

                let body_entry = cfg.add_node(NodeKind::Statement);
                cfg.add_edge(condition, body_entry);

                stack.push(Task::AfterWhileBody { condition });
                stack.push(Task::Block {
                    stmts: &expr_while.body.stmts,
                    idx: 0,
                    entry: body_entry,
                    exit: condition,
                });
            }
            Task::AfterWhileBody { condition } => {
                let body_exit = last;
                // Back edge to condition
                cfg.add_edge(body_exit, condition);

                // Loop exit
                let loop_exit = cfg.add_node(NodeKind::Join);
                cfg.add_edge(condition, loop_exit);
                last = loop_exit;
            }
            Task::For { expr_for, entry } => {
                let condition = cfg.add_node(NodeKind::Condition);
                cfg.add_edge(entry, condition);

                let body_entry = cfg.add_node(NodeKind::Statement);
                cfg.add_edge(condition, body_entry);

                stack.push(Task::AfterForBody { condition });
                stack.push(Task::Block {
                    stmts: &expr_for.body.stmts,
                    idx: 0,
                    entry: body_entry,
                    exit: condition,
                });
            }
            Task::AfterForBody { condition } => {
                let body_exit = last;
                // Back edge to condition
                cfg.add_edge(body_exit, condition);

                // Loop exit
                let loop_exit = cfg.add_node(NodeKind::Join);
                cfg.add_edge(condition, loop_exit);
                last = loop_exit;
            }
            Task::ExprBlock {
                expr_block,
                entry,
                exit,
            } => {
                stack.push(Task::Block {
                    stmts: &expr_block.block.stmts,
                    idx: 0,
                    entry,
                    exit,
                });
            }
        }
    }

    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::FunctionId;
    use crate::language::{FunctionBody, SourceSpan};

    fn make_test_function(source: &str) -> FunctionNode {
        FunctionNode {
            id: FunctionId {
                file_index: 0,
                local_index: 0,
            },
            name: Some("test".to_string()),
            span: SourceSpan::new(0, source.len(), 1, 1, 0),
            body: FunctionBody::Rust {
                source: source.to_string(),
            },
            suppression_reason: None,
        }
    }

    #[test]
    fn test_rust_cfg_builder_deeply_nested_does_not_overflow_and_builds_real_cfg() {
        // Build a deeply right-nested `Block` tree directly, bypassing
        // `syn::parse_str`. `syn`'s own recursive-descent parser has no depth
        // bound either and overflows a generous stack well below the depth
        // this test uses (that's a separate, still-open gap — see this
        // file's module doc) — going through source text would end up
        // testing the parser's recursion limit, not the CFG builder's. This
        // isolates exactly the component this change replaced: previously
        // `build_block_cfg`/`build_expr_cfg` recursed natively per level and
        // depended on `MAX_CFG_DEPTH` (500) to avoid overflowing the calling
        // thread's stack; the iterative builder has no native recursion at
        // all, so it should handle depth far beyond that old limit even on a
        // thread stack far smaller than the old guard required.
        const DEPTH: usize = 50_000;

        // `Expr::Block` wrapping is a no-op for the builder (`Task::ExprBlock`
        // just unwraps straight into the inner block, adding no node) — plain
        // nested blocks wouldn't actually exercise the per-level work this
        // builder does. Nest `if` expressions instead: each level pushes an
        // `If` task, and its continuation (`AfterIfThen`) adds a condition and
        // join node, exactly the per-level stack growth that used to be
        // native recursion bounded by `MAX_CFG_DEPTH`.
        fn nested_if(depth: usize, cond: &Expr) -> Block {
            let mut block = Block {
                brace_token: syn::token::Brace::default(),
                stmts: vec![Stmt::Expr(
                    Expr::Break(syn::ExprBreak {
                        attrs: Vec::new(),
                        break_token: Default::default(),
                        label: None,
                        expr: None,
                    }),
                    None,
                )],
            };
            for _ in 0..depth {
                let if_expr = Expr::If(ExprIf {
                    attrs: Vec::new(),
                    if_token: Default::default(),
                    cond: Box::new(cond.clone()),
                    then_branch: block,
                    else_branch: None,
                });
                block = Block {
                    brace_token: syn::token::Brace::default(),
                    stmts: vec![Stmt::Expr(if_expr, None)],
                };
            }
            block
        }

        // `syn::Block` (via `proc_macro2::Span`) isn't `Send`, so build it and
        // run the CFG builder on the same spawned thread.
        let node_count = std::thread::Builder::new()
            .stack_size(256 * 1024) // far below MAX_CFG_DEPTH's required stack
            .spawn(move || {
                let cond: Expr = syn::parse_str("true").unwrap();
                let block = nested_if(DEPTH, &cond);
                let mut cfg = Cfg::new();
                let entry = cfg.entry;
                let exit = cfg.exit;
                let last = build_block_cfg(&mut cfg, &block, entry, exit);
                cfg.add_edge(last, exit);
                // `syn::Block`'s generated `Drop` glue recurses per nesting
                // level just like its parser does (both are properties of
                // `syn`'s AST shape, not of this builder — see the module
                // doc's note on the still-open parser-depth gap). Skip it so
                // this test isolates what actually changed: `build_block_cfg`
                // itself no longer recurses natively.
                std::mem::forget(block);
                cfg.node_count()
            })
            .unwrap()
            .join()
            .unwrap();

        // Each nesting level adds one Statement node (an `Expr::Block`
        // wrapping the next level in); a minimal fallback CFG would be
        // exactly 2 (entry, exit) and this asserts far more than that.
        assert!(
            node_count > DEPTH,
            "expected a fully-built CFG with >{DEPTH} nodes, got {node_count}"
        );
    }

    #[test]
    fn test_rust_cfg_builder_simple() {
        let source = r#"
fn simple() {
    let x = 1;
}
"#;

        let function = make_test_function(source);
        let builder = RustCfgBuilder;
        let cfg = builder.build(&function);

        // Should have entry, exit, and statement nodes
        assert!(cfg.node_count() >= 2);
        assert_eq!(cfg.entry, NodeId(0));
        assert_eq!(cfg.exit, NodeId(1));
    }

    #[test]
    fn test_rust_cfg_builder_if() {
        let source = r#"
fn with_if(x: i32) {
    if x > 0 {
        println!("positive");
    }
}
"#;

        let function = make_test_function(source);
        let builder = RustCfgBuilder;
        let cfg = builder.build(&function);

        // Should have entry, exit, condition, and branches
        assert!(cfg.node_count() >= 4);
    }

    #[test]
    fn test_rust_cfg_builder_if_else() {
        let source = r#"
fn with_if_else(x: i32) {
    if x > 0 {
        println!("positive");
    } else {
        println!("non-positive");
    }
}
"#;

        let function = make_test_function(source);
        let builder = RustCfgBuilder;
        let cfg = builder.build(&function);

        assert!(cfg.node_count() >= 5);
    }

    #[test]
    fn test_rust_cfg_builder_match() {
        let source = r#"
fn with_match(x: i32) {
    match x {
        0 => println!("zero"),
        1 => println!("one"),
        _ => println!("other"),
    }
}
"#;

        let function = make_test_function(source);
        let builder = RustCfgBuilder;
        let cfg = builder.build(&function);

        // Should have entry, exit, condition, and match arms
        assert!(cfg.node_count() >= 4);
    }

    #[test]
    fn test_rust_cfg_builder_nested_if_in_loop() {
        // Exercises the stack correctly unwinding nested constructs across
        // multiple Task variants (Loop -> Block -> If -> Block).
        let source = r#"
fn nested(x: i32) {
    loop {
        if x > 0 {
            break;
        }
    }
}
"#;

        let function = make_test_function(source);
        let builder = RustCfgBuilder;
        let cfg = builder.build(&function);

        assert!(cfg.node_count() >= 5);
    }
}
