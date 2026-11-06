//! Helpers for function ids of the form `{file}::{name}`.
//!
//! File paths never contain `::`, but names can: C++ qualified names (`ns::Class::method`)
//! and overload signatures (`f(std::string&)`). Splitting an id at the *last* `::` therefore
//! mistakes part of the name for the file, or returns a fragment of a signature as the name.
//! Always go through these helpers instead of splitting on `::` directly.

/// Split an id into `(file, name)` at the first `::`.
pub fn split(id: &str) -> Option<(&str, &str)> {
    id.split_once("::")
}

/// The file part of an id, if it has one.
pub fn file_of(id: &str) -> Option<&str> {
    split(id).map(|(file, _)| file)
}

/// The full name part of an id (qualified, with any overload signature), or the whole id
/// when it has no file part.
pub fn name_of(id: &str) -> &str {
    split(id).map(|(_, name)| name).unwrap_or(id)
}

/// A short display name: the name part without an overload signature and without its
/// namespace/class qualifiers. `src/a.cc::ns::C::f(std::string&) const` gives `f`.
pub fn short_name(id: &str) -> &str {
    last_component(strip_signature(name_of(id)))
}

/// Remove a trailing overload signature added to disambiguate C++ overloads:
/// `f(int,char*) const`, `f(int)@12`. `operator()` itself is not a signature.
pub fn strip_signature(name: &str) -> &str {
    let mut s = name;
    if let Some(i) = s.rfind('@') {
        let tail = &s[i + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            s = &s[..i];
        }
    }
    for q in [" const", " volatile"] {
        while let Some(rest) = s.strip_suffix(q) {
            s = rest;
        }
    }
    if !s.ends_with(')') {
        return s;
    }
    // Find the '(' matching the final ')'.
    let mut depth = 0usize;
    for (i, c) in s.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    let prefix = &s[..i];
                    // `operator()` is a name, not an empty signature.
                    return if prefix.ends_with("operator") || prefix.is_empty() {
                        s
                    } else {
                        prefix
                    };
                }
            }
            _ => {}
        }
    }
    s
}

/// The part after the last top-level `::` (ignoring any inside `<...>` or `(...)`).
fn last_component(name: &str) -> &str {
    let bytes = name.as_bytes();
    let mut depth = 0usize;
    let mut cut = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth = depth.saturating_sub(1),
            b':' if depth == 0 && bytes.get(i + 1) == Some(&b':') => {
                cut = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    &name[cut..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ids_behave_as_before() {
        assert_eq!(file_of("src/a.ts::handler"), Some("src/a.ts"));
        assert_eq!(short_name("src/a.ts::handler"), "handler");
        assert_eq!(short_name("handler"), "handler");
    }

    #[test]
    fn qualified_names_keep_the_file_intact() {
        let id = "src/db/segment.cc::zvec::SegmentImpl::alter_column";
        assert_eq!(file_of(id), Some("src/db/segment.cc"));
        assert_eq!(name_of(id), "zvec::SegmentImpl::alter_column");
        assert_eq!(short_name(id), "alter_column");
    }

    #[test]
    fn overload_signatures_do_not_leak_into_the_short_name() {
        let id = "a.cc::zvec::S::fetch(uint64_t,std::optional<std::vector<std::string>>&,bool)";
        assert_eq!(short_name(id), "fetch");
        assert_eq!(short_name("a.cc::f(std::string&) const"), "f");
        assert_eq!(short_name("a.cc::f(int)@12"), "f");
        assert_eq!(short_name("a.cc::f(void(*)(int),char)"), "f");
    }

    #[test]
    fn operators_and_destructors() {
        assert_eq!(short_name("a.cc::Foo::operator()"), "operator()");
        assert_eq!(short_name("a.cc::Foo::operator()(int)"), "operator()");
        assert_eq!(short_name("a.cc::Foo::operator=="), "operator==");
        assert_eq!(short_name("a.cc::Foo::operator<<"), "operator<<");
        assert_eq!(short_name("a.cc::Foo::~Foo"), "~Foo");
    }

    #[test]
    fn template_arguments_are_not_split() {
        assert_eq!(short_name("a.cc::Box<std::string>::get"), "get");
    }
}
