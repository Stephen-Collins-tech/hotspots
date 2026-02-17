use tree_sitter::Node;

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
