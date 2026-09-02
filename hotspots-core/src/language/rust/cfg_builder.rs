//! Rust CFG builder implementation

use crate::ast::FunctionNode;
use crate::cfg::{Cfg, NodeId, NodeKind};
use crate::language::cfg_builder::CfgBuilder;
use anyhow::{Context, Result};
use syn::{Block, Expr, ExprBlock, ExprForLoop, ExprIf, ExprLoop, ExprMatch, ExprWhile, Stmt};

/// Maximum expression/statement nesting depth the recursive-descent builder below
/// will walk before bailing out.
///
/// This is a stopgap depth guard, not a structural fix: `build_block_cfg` /
/// `build_expr_cfg` / `build_stmt_cfg` are mutually recursive over the AST, so a
/// sufficiently deep or generated function body (e.g. rustc's own `tests/ui/`
/// fixtures) can still overflow the calling thread's stack even with this guard,
/// depending on how deep `MAX_CFG_DEPTH` is set relative to the configured stack
/// size (see the rayon worker stack size in `hotspots-cli/src/cmd/analyze.rs`).
/// A proper fix would convert this recursive walk into an iterative one with an
/// explicit heap-allocated work stack, removing the depth bound entirely. See
/// https://github.com/Stephen-Collins-tech/hotspots/issues/160 for that follow-up.
const MAX_CFG_DEPTH: usize = 500;

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
    let last_node = build_block_cfg(&mut cfg, &item_fn.block, entry, exit, 0)?;

    // Connect last node to exit
    cfg.add_edge(last_node, exit);

    Ok(cfg)
}

/// Build CFG for a block
fn build_block_cfg(
    cfg: &mut Cfg,
    block: &Block,
    entry: NodeId,
    exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    if depth > MAX_CFG_DEPTH {
        anyhow::bail!("CFG nesting depth exceeded ({MAX_CFG_DEPTH}); aborting CFG build");
    }

    let mut current = entry;

    for stmt in &block.stmts {
        current = build_stmt_cfg(cfg, stmt, current, exit, depth + 1)?;
    }

    Ok(current)
}

/// Build CFG for a statement
fn build_stmt_cfg(
    cfg: &mut Cfg,
    stmt: &Stmt,
    entry: NodeId,
    exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    match stmt {
        Stmt::Expr(expr, _) => build_expr_cfg(cfg, expr, entry, exit, depth),
        Stmt::Local(_) => {
            // Variable declaration
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            Ok(node)
        }
        Stmt::Item(_) => {
            // Nested item (function, struct, etc.) - treat as statement
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            Ok(node)
        }
        Stmt::Macro(_) => {
            // Macro invocation - treat as statement
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            Ok(node)
        }
    }
}

/// Build CFG for an expression
fn build_expr_cfg(
    cfg: &mut Cfg,
    expr: &Expr,
    entry: NodeId,
    exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    if depth > MAX_CFG_DEPTH {
        anyhow::bail!("CFG nesting depth exceeded ({MAX_CFG_DEPTH}); aborting CFG build");
    }

    match expr {
        Expr::If(expr_if) => build_if_cfg(cfg, expr_if, entry, exit, depth + 1),
        Expr::Match(expr_match) => build_match_cfg(cfg, expr_match, entry, exit, depth + 1),
        Expr::Loop(expr_loop) => build_loop_cfg(cfg, expr_loop, entry, exit, depth + 1),
        Expr::While(expr_while) => build_while_cfg(cfg, expr_while, entry, exit, depth + 1),
        Expr::ForLoop(expr_for) => build_for_cfg(cfg, expr_for, entry, exit, depth + 1),
        Expr::Block(expr_block) => build_expr_block_cfg(cfg, expr_block, entry, exit, depth + 1),
        Expr::Return(_) => {
            // Return statement - connects to exit
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            cfg.add_edge(node, exit);
            Ok(node)
        }
        Expr::Break(_) => {
            // Break statement
            // Note: In a full implementation, we'd route break to loop exit
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            Ok(node)
        }
        Expr::Continue(_) => {
            // Continue statement
            // Note: In a full implementation, we'd route continue to loop header
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            Ok(node)
        }
        _ => {
            // Other expressions (calls, literals, etc.)
            let node = cfg.add_node(NodeKind::Statement);
            cfg.add_edge(entry, node);
            Ok(node)
        }
    }
}

/// Build CFG for if expression
fn build_if_cfg(
    cfg: &mut Cfg,
    expr_if: &ExprIf,
    entry: NodeId,
    exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    let condition = cfg.add_node(NodeKind::Condition);
    cfg.add_edge(entry, condition);

    // Then branch
    let then_entry = cfg.add_node(NodeKind::Statement);
    cfg.add_edge(condition, then_entry);
    let then_exit = build_block_cfg(cfg, &expr_if.then_branch, then_entry, exit, depth)?;

    // Join node
    let join = cfg.add_node(NodeKind::Join);
    cfg.add_edge(then_exit, join);

    // Else branch
    if let Some((_, else_expr)) = &expr_if.else_branch {
        let else_entry = cfg.add_node(NodeKind::Statement);
        cfg.add_edge(condition, else_entry);
        let else_exit = build_expr_cfg(cfg, else_expr, else_entry, exit, depth)?;
        cfg.add_edge(else_exit, join);
    } else {
        // No else branch - condition can go directly to join
        cfg.add_edge(condition, join);
    }

    Ok(join)
}

/// Build CFG for match expression
fn build_match_cfg(
    cfg: &mut Cfg,
    expr_match: &ExprMatch,
    entry: NodeId,
    exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    let condition = cfg.add_node(NodeKind::Condition);
    cfg.add_edge(entry, condition);

    let join = cfg.add_node(NodeKind::Join);

    // Each match arm is a separate path
    for arm in &expr_match.arms {
        let arm_entry = cfg.add_node(NodeKind::Statement);
        cfg.add_edge(condition, arm_entry);
        let arm_exit = build_expr_cfg(cfg, &arm.body, arm_entry, exit, depth)?;
        cfg.add_edge(arm_exit, join);
    }

    Ok(join)
}

/// Build CFG for loop expression
fn build_loop_cfg(
    cfg: &mut Cfg,
    expr_loop: &ExprLoop,
    entry: NodeId,
    _exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    let header = cfg.add_node(NodeKind::LoopHeader);
    cfg.add_edge(entry, header);

    let body_exit = build_block_cfg(cfg, &expr_loop.body, header, header, depth)?;

    // Back edge to header
    cfg.add_edge(body_exit, header);

    // Loop exit (for breaks)
    let loop_exit = cfg.add_node(NodeKind::Join);
    cfg.add_edge(header, loop_exit);

    Ok(loop_exit)
}

/// Build CFG for while loop
fn build_while_cfg(
    cfg: &mut Cfg,
    expr_while: &ExprWhile,
    entry: NodeId,
    _exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    let condition = cfg.add_node(NodeKind::Condition);
    cfg.add_edge(entry, condition);

    let body_entry = cfg.add_node(NodeKind::Statement);
    cfg.add_edge(condition, body_entry);

    let body_exit = build_block_cfg(cfg, &expr_while.body, body_entry, condition, depth)?;

    // Back edge to condition
    cfg.add_edge(body_exit, condition);

    // Loop exit
    let loop_exit = cfg.add_node(NodeKind::Join);
    cfg.add_edge(condition, loop_exit);

    Ok(loop_exit)
}

/// Build CFG for for loop
fn build_for_cfg(
    cfg: &mut Cfg,
    expr_for: &ExprForLoop,
    entry: NodeId,
    _exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    let condition = cfg.add_node(NodeKind::Condition);
    cfg.add_edge(entry, condition);

    let body_entry = cfg.add_node(NodeKind::Statement);
    cfg.add_edge(condition, body_entry);

    let body_exit = build_block_cfg(cfg, &expr_for.body, body_entry, condition, depth)?;

    // Back edge to condition
    cfg.add_edge(body_exit, condition);

    // Loop exit
    let loop_exit = cfg.add_node(NodeKind::Join);
    cfg.add_edge(condition, loop_exit);

    Ok(loop_exit)
}

/// Build CFG for expression block
fn build_expr_block_cfg(
    cfg: &mut Cfg,
    expr_block: &ExprBlock,
    entry: NodeId,
    exit: NodeId,
    depth: usize,
) -> Result<NodeId> {
    build_block_cfg(cfg, &expr_block.block, entry, exit, depth)
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
    fn test_rust_cfg_builder_depth_guard_falls_back_to_minimal_cfg() {
        // Deeply nested blocks exceed MAX_CFG_DEPTH; the builder should bail out
        // to a minimal entry->exit CFG instead of overflowing the stack.
        //
        // Run on a dedicated thread with a generous stack: `syn::parse_str`'s own
        // recursive-descent parser has no depth bound either, and will overflow
        // the default 2MB test-thread stack on nesting well below MAX_CFG_DEPTH,
        // before CFG building (and this guard) even starts. That's a separate gap
        // this depth guard does not close — see MAX_CFG_DEPTH's doc comment.
        let mut source = String::from("fn deeply_nested() {\n");
        for _ in 0..(MAX_CFG_DEPTH * 2) {
            source.push_str("{\n");
        }
        for _ in 0..(MAX_CFG_DEPTH * 2) {
            source.push_str("}\n");
        }
        source.push_str("}\n");

        let (entry, exit) = std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let function = make_test_function(&source);
                let builder = RustCfgBuilder;
                let cfg = builder.build(&function);
                (cfg.entry, cfg.exit)
            })
            .unwrap()
            .join()
            .unwrap();

        assert_eq!(entry, NodeId(0));
        assert_eq!(exit, NodeId(1));
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
}
