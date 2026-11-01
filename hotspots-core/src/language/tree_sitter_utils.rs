use std::cell::RefCell;
use std::sync::Arc;
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
        // A match starts at `start_byte`, so skip subtrees whose range cannot contain it.
        if child.start_byte() > start_byte || child.end_byte() <= start_byte {
            continue;
        }
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

/// Macro that generates a thread-local parse cache and a `with_cached_*_tree`
/// accessor for a given tree-sitter language.
///
/// The cache is keyed by the identity of the shared source `Arc` (every function in a file holds a
/// clone of the same one), not by hashing the source: hashing a multi-megabyte file once per
/// function was O(functions x file size). The cache keeps its own clone of the `Arc`, which pins the
/// allocation, so the address cannot be reused by a different source while it is cached and a hit
/// always means the same bytes.
macro_rules! make_parse_cache {
    ($cache:ident, $with_fn:ident, $lang_expr:expr) => {
        thread_local! {
            static $cache: RefCell<Option<(Arc<str>, Tree)>> = const { RefCell::new(None) };
        }

        /// Parse `source` with the language, caching the result.
        /// Calls `f` with the root node while the cached tree is borrowed.
        pub fn $with_fn<F, R>(source: &Arc<str>, f: F) -> Option<R>
        where
            F: for<'a> FnOnce(Node<'a>) -> Option<R>,
        {
            $cache.with(|cache| {
                let mut c = cache.borrow_mut();
                let needs_parse = match &*c {
                    Some((cached, _)) => !Arc::ptr_eq(cached, source),
                    None => true,
                };
                if needs_parse {
                    let mut parser = tree_sitter::Parser::new();
                    parser.set_language(&$lang_expr.into()).ok()?;
                    let tree = parser.parse(&**source, None)?;
                    *c = Some((Arc::clone(source), tree));
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

make_parse_cache!(
    CSHARP_TREE_CACHE,
    with_cached_csharp_tree,
    tree_sitter_c_sharp::LANGUAGE
);

make_parse_cache!(C_TREE_CACHE, with_cached_c_tree, tree_sitter_c::LANGUAGE);

#[cfg(test)]
mod parse_cache_tests {
    use super::*;

    fn root_len(source: &Arc<str>) -> usize {
        with_cached_go_tree(source, |root| Some(root.end_byte())).unwrap()
    }

    #[test]
    fn same_shared_source_reuses_the_cached_tree() {
        let src: Arc<str> = Arc::from("package main\nfunc a() {}\n");
        assert_eq!(root_len(&src), src.len());
        assert_eq!(root_len(&Arc::clone(&src)), src.len());
    }

    #[test]
    fn a_different_source_never_gets_a_stale_tree() {
        // The first source is dropped by the caller while the cache still holds it; the second,
        // different source must be parsed fresh even if it has the same length.
        let first_len = {
            let first: Arc<str> = Arc::from("package main\nfunc a() {}\n");
            root_len(&first)
        };
        let second: Arc<str> = Arc::from("package main\nfunc b() {}\nfunc c() {}\n");
        assert_ne!(second.len(), first_len);
        assert_eq!(root_len(&second), second.len());

        let same_len_a: Arc<str> = Arc::from("package main\nfunc a() {}\n");
        let same_len_b: Arc<str> = Arc::from("package main\nfunc z() {}\n");
        assert_eq!(same_len_a.len(), same_len_b.len());
        let name_of = |s: &Arc<str>| {
            with_cached_go_tree(s, |root| {
                let f = find_function_by_start(root, 13, &["function_declaration"])?;
                let n = find_child_by_kind(f, "identifier")?;
                Some(s[n.start_byte()..n.end_byte()].to_string())
            })
            .unwrap()
        };
        assert_eq!(name_of(&same_len_a), "a");
        assert_eq!(name_of(&same_len_b), "z");
    }
}
