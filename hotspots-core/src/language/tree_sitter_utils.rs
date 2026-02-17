use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use tree_sitter::{Node, Tree};

pub fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}

pub fn find_function_by_start<'a>(
    node: Node<'a>,
    start_byte: usize,
    func_kinds: &[&str],
) -> Option<Node<'a>> {
    if func_kinds.contains(&node.kind()) && node.start_byte() == start_byte {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_by_start(child, start_byte, func_kinds) {
            return Some(found);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Per-language parse caches
//
// Each cache holds the most recently parsed tree for that language. Because
// all functions in a file are analyzed sequentially and share the same source
// string, every function after the first gets a cache hit — reducing O(n × m)
// parses to O(1) per file.
//
// The callback pattern (`with_cached_*_tree`) sidesteps the tree-sitter
// lifetime problem: Node<'_> borrows from Tree, which lives inside the
// RefCell. By doing all node work inside the closure while the RefMut is
// alive, the borrow checker is satisfied without any unsafe code.
// ---------------------------------------------------------------------------

fn hash_source(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Macro that generates a thread-local parse cache and a `with_cached_*_tree`
/// accessor for a given tree-sitter language.
macro_rules! make_parse_cache {
    ($cache:ident, $with_fn:ident, $lang_expr:expr) => {
        thread_local! {
            static $cache: RefCell<Option<(u64, Tree)>> = const { RefCell::new(None) };
        }

        /// Parse `source` with the language, caching the result.
        /// Calls `f` with the root node while the cached tree is borrowed.
        pub fn $with_fn<F, R>(source: &str, f: F) -> Option<R>
        where
            F: for<'a> FnOnce(Node<'a>) -> Option<R>,
        {
            let hash = hash_source(source);
            $cache.with(|cache| {
                let mut c = cache.borrow_mut();
                let needs_parse = match &*c {
                    Some((h, _)) => *h != hash,
                    None => true,
                };
                if needs_parse {
                    let mut parser = tree_sitter::Parser::new();
                    parser.set_language(&$lang_expr.into()).ok()?;
                    let tree = parser.parse(source, None)?;
                    *c = Some((hash, tree));
                }
                let root = c.as_ref()?.1.root_node();
                f(root)
            })
        }
    };
}

make_parse_cache!(GO_TREE_CACHE, with_cached_go_tree, tree_sitter_go::LANGUAGE);

make_parse_cache!(
    JAVA_TREE_CACHE,
    with_cached_java_tree,
    tree_sitter_java::LANGUAGE
);

make_parse_cache!(
    PYTHON_TREE_CACHE,
    with_cached_python_tree,
    tree_sitter_python::LANGUAGE
);
