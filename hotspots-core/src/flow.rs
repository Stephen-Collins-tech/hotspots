//! Rewrites Flow-only syntax into valid TypeScript so the TypeScript grammar can parse it.
//!
//! Flow-annotated JavaScript (`// @flow`) is mostly valid TypeScript already; this handles the
//! common forms that are not. Every rewrite keeps the byte length, so line and column positions
//! in the rewritten text match the original. The result is only fed to the parser to find
//! functions and control flow; type meaning is irrelevant, so a rewrite may change what a type
//! means (a nullable `?T` loses its `?`).
//!
//! Comments and string literals are copied through untouched.

/// Rewrite the common Flow-only forms in `src`. Same length as the input.
pub fn flow_to_typescript(src: &str) -> String {
    rewrite(src, false)
}

/// Like [`flow_to_typescript`], and also blanks object-type spreads (`...Props,`) and the
/// inexact-object marker (`...`). A value spread is blanked too, which does not change control
/// flow, but callers should try [`flow_to_typescript`] first so files that parse without it keep
/// every spread.
pub fn flow_to_typescript_aggressive(src: &str) -> String {
    rewrite(src, true)
}

fn rewrite(src: &str, aggressive: bool) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let n = b.len();
    let mut i = 0;
    // Previous significant (non-space, non-comment) byte and the one before it.
    let (mut p1, mut p2) = (0u8, 0u8);
    // For each open `{`: did it open in a type-like position (after `=`, `:`, `<`, `(`, `,`, `|`,
    // `&`)? A `{` after an identifier or `}` is a JSX attribute spread or a block, where a spread
    // must not be touched.
    let mut braces: Vec<bool> = Vec::new();
    let mut last_word: &str = "";

    while i < n {
        let c = b[i];
        // Comments and strings are skipped as a unit.
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }
        if c == b'"' || c == b'\'' || c == b'`' {
            i += 1;
            while i < n && b[i] != c {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            (p2, p1) = (p1, c);
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        match c {
            // Exact object types: `{| a: T |}`.
            b'{' => {
                braces.push(
                    matches!(p1, b'=' | b':' | b'<' | b'(' | b',' | b'|' | b'&')
                        || (is_ident_char(p1) && matches!(last_word, "extends" | "keyof" | "as")),
                );
                if i + 1 < n && b[i + 1] == b'|' {
                    out[i + 1] = b' ';
                    i += 2;
                    (p2, p1) = (b'{', b'{');
                    continue;
                }
            }
            b'}' => {
                braces.pop();
            }
            b'|' if i + 1 < n && b[i + 1] == b'}' => {
                out[i] = b' ';
                braces.pop();
                i += 2;
                (p2, p1) = (b'|', b'}');
                continue;
            }
            // Nullable types: `: ?T`, `<?T>`, `(?T)`, `, ?T`, `| ?T`, `=> ?T`. A `?` here cannot
            // be the conditional operator or optional chaining, which follow an expression.
            b'?' if (is_nullable_position(p1, p2) || prev_word_is_as(b, i))
                && next_starts_type(b, i + 1) =>
            {
                out[i] = b' ';
            }
            // `...` inexact marker and `...Type,` spreads inside object types.
            b'.' if aggressive
                && braces.last().copied().unwrap_or(false)
                && b[i..].starts_with(b"...") =>
            {
                let end = spread_end(b, i + 3);
                if let Some(end) = end {
                    blank(&mut out, i, end);
                    i = end;
                    (p2, p1) = (b' ', b' ');
                    continue;
                }
            }
            // Variance sigils on object type properties: `+x: T`, `-x: T`.
            b'+' | b'-' if matches!(p1, b'{' | b',' | b';') && looks_like_property(b, i + 1) => {
                out[i] = b' ';
            }
            _ => {}
        }

        if is_ident_start(c) {
            let start = i;
            while i < n && is_ident_char(b[i]) {
                i += 1;
            }
            let word = &src[start..i];
            last_word = word;
            match word {
                // Flow component declarations: `component Name(props) { ... }` is a function.
                "component" if followed_by_ident_then_paren(b, i) && starts_statement(b, start) => {
                    out[start..i].copy_from_slice(b"function ");
                }
                // `opaque type T = ...` and `export opaque type`.
                "opaque" if followed_by_word(b, i, "type") => blank(&mut out, start, i),
                // `import typeof X from` -> `import type  X from` (same length).
                "typeof" if prev_word_is_import(b, start) => {
                    out[start..i].copy_from_slice(b"type  ");
                }
                _ => {}
            }
            (p2, p1) = (b[i - 1], b[i - 1]);
            continue;
        }
        // `%checks` after a function signature.
        if c == b'%' && src[i..].starts_with("%checks") {
            blank(&mut out, i, i + 7);
            i += 7;
            continue;
        }

        (p2, p1) = (p1, c);
        i += 1;
    }
    // Only ASCII bytes were replaced, by ASCII bytes, so this is still valid UTF-8.
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

fn blank(out: &mut [u8], from: usize, to: usize) {
    for byte in &mut out[from..to] {
        *byte = b' ';
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// True if a `?` after `p1` (and before it `p2`) can start a Flow nullable type.
fn is_nullable_position(p1: u8, p2: u8) -> bool {
    match p1 {
        b':' | b'<' | b',' | b'(' | b'|' | b'&' | b'=' => true,
        // `=> ?T`
        b'>' => p2 == b'=',
        _ => false,
    }
}

/// The byte after `?` must begin a type, not `.` (optional chaining) or `?` (nullish).
fn next_starts_type(b: &[u8], mut i: usize) -> bool {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i < b.len() && (is_ident_start(b[i]) || matches!(b[i], b'(' | b'{' | b'[' | b'\'' | b'"'))
}

/// `+name:` or `+name?:` inside an object type.
fn looks_like_property(b: &[u8], mut i: usize) -> bool {
    let s = i;
    while i < b.len() && is_ident_char(b[i]) {
        i += 1;
    }
    if i == s {
        return false;
    }
    if i < b.len() && b[i] == b'?' {
        i += 1;
    }
    i < b.len() && b[i] == b':'
}

/// True if the word right before position `i` is `as` (`x as ?T`).
fn prev_word_is_as(b: &[u8], i: usize) -> bool {
    let mut j = i;
    while j > 0 && b[j - 1] == b' ' {
        j -= 1;
    }
    j >= 2 && &b[j - 2..j] == b"as" && (j == 2 || !is_ident_char(b[j - 3])) && j < i
}

/// End (exclusive) of a `...` spread or inexact marker starting after the dots at `i`,
/// including a trailing comma. `None` if it is something else (for example a call spread).
fn spread_end(b: &[u8], mut i: usize) -> Option<usize> {
    let skip_ws = |mut k: usize| {
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        k
    };
    i = skip_ws(i);
    if i < b.len() && (is_ident_start(b[i])) {
        // A dotted identifier path, optionally with type arguments: `A.B<C>`.
        loop {
            while i < b.len() && is_ident_char(b[i]) {
                i += 1;
            }
            if i + 1 < b.len() && b[i] == b'.' && is_ident_start(b[i + 1]) {
                i += 1;
            } else {
                break;
            }
        }
        if i < b.len() && b[i] == b'<' {
            let mut depth = 0usize;
            while i < b.len() {
                match b[i] {
                    b'<' => depth += 1,
                    b'>' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        i = skip_ws(i);
    }
    match b.get(i) {
        Some(b',') | Some(b';') => Some(i + 1),
        Some(b'}') => Some(i),
        _ => None,
    }
}

/// `<ws>Name(`: an identifier directly followed by an opening parenthesis.
fn followed_by_ident_then_paren(b: &[u8], mut i: usize) -> bool {
    let ws = i;
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if i == ws || i >= b.len() || !is_ident_start(b[i]) {
        return false;
    }
    while i < b.len() && is_ident_char(b[i]) {
        i += 1;
    }
    i < b.len() && b[i] == b'('
}

/// True if the word at `start` begins a statement: first on its line or after `export` /
/// `export default`, not a member or variable named `component`.
fn starts_statement(b: &[u8], start: usize) -> bool {
    let mut j = start;
    while j > 0 && (b[j - 1] == b' ' || b[j - 1] == b'\t') {
        j -= 1;
    }
    if j == 0 || b[j - 1] == b'\n' || b[j - 1] == b';' || b[j - 1] == b'}' {
        return true;
    }
    let before = &b[..j];
    before.ends_with(b"export") || before.ends_with(b"default")
}

fn followed_by_word(b: &[u8], mut i: usize, word: &str) -> bool {
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    b[i..].starts_with(word.as_bytes())
        && b.get(i + word.len()).map_or(true, |c| !is_ident_char(*c))
}

fn prev_word_is_import(b: &[u8], start: usize) -> bool {
    let mut j = start;
    while j > 0 && b[j - 1] == b' ' {
        j -= 1;
    }
    j >= 6 && &b[j - 6..j] == b"import"
}

#[cfg(test)]
mod tests {
    use super::flow_to_typescript as f;

    #[test]
    fn keeps_length() {
        let s = "// @flow\ntype A = {| a: ?string, +b: number |};\nopaque type T = number;\n";
        assert_eq!(f(s).len(), s.len());
    }

    #[test]
    fn nullable_types_become_unions() {
        assert_eq!(f("let x: ?string;"), "let x:  string;");
        assert_eq!(f("type A = Array<?T>;"), "type A = Array< T>;");
        assert_eq!(f("type F = () => ?T;"), "type F = () =>  T;");
        assert_eq!(f("let y = x as ?T;"), "let y = x as  T;");
    }

    #[test]
    fn ternary_and_optional_chaining_untouched() {
        let s = "const a = b ? c : d; const e = f?.g; const h = i ?? j;";
        assert_eq!(f(s), s);
    }

    #[test]
    fn exact_objects_opaque_import_typeof_checks() {
        assert_eq!(f("type A = {| a: number |};"), "type A = {  a: number  };");
        assert_eq!(f("opaque type T = number;"), "       type T = number;");
        assert_eq!(f("import typeof X from 'x';"), "import type   X from 'x';");
        assert_eq!(
            f("function f(x): boolean %checks {}"),
            "function f(x): boolean         {}"
        );
    }

    #[test]
    fn aggressive_blanks_type_spreads_and_inexact_marker_only() {
        use super::flow_to_typescript_aggressive as g;
        let collapse = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
        for (src, want) in [
            ("type A = {...Props, a: T};", "type A = {a: T};"),
            ("type A = {a: T, ...};", "type A = {a: T,};"),
            ("type A = {...$Exact<B>};", "type A = {};"),
            ("type A = {...Mod.Props<B>, c: U};", "type A = {c: U};"),
        ] {
            let out = g(src);
            assert_eq!(out.len(), src.len(), "{src}");
            assert_eq!(
                collapse(out).replace("{ ", "{").replace(" }", "}"),
                want,
                "{src}"
            );
        }
        // The safe pass leaves spreads alone, and a call spread is never touched.
        assert_eq!(f("type A = {...P};"), "type A = {...P};");
        assert_eq!(g("f(...args);"), "f(...args);");
    }

    #[test]
    fn component_declarations_become_functions() {
        let src = "export default component View(ref?: R, ...props: P) {\n  return 1;\n}\n";
        let out = f(src);
        assert_eq!(out.len(), src.len());
        assert!(out.starts_with("export default function  View(") || out.contains("function View"));
        // A variable or member called `component` is left alone.
        let keep = "const component = x; foo.component(y);";
        assert_eq!(f(keep), keep);
    }

    #[test]
    fn inexact_marker_after_extends_is_blanked_by_the_aggressive_pass() {
        let out = super::flow_to_typescript_aggressive("type A<T> = T extends {...} ? 1 : 2;");
        assert!(!out.contains("..."));
    }

    #[test]
    fn variance_sigils_blanked() {
        assert_eq!(
            f("type A = {+a: number, -b: string};"),
            "type A = { a: number,  b: string};"
        );
    }

    #[test]
    fn strings_and_comments_untouched() {
        let s = "// x: ?T\nconst a = \"y: ?T\"; /* {| |} */";
        assert_eq!(f(s), s);
    }
}
