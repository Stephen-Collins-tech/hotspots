//! Import extraction and file-level dependency resolution
//!
//! Parses `use`/`import` statements from source files and resolves them to
//! in-project file paths. Used by `aggregates.rs` to compute module instability.
//!
//! Global invariants:
//! - Resolution is best-effort: unresolved imports produce no edge (never wrong)
//! - External library imports are silently dropped
//! - Does NOT modify the existing call graph

use crate::language::Language;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Extract raw, unresolved import paths from source code.
///
/// Returns paths as written in the source (module paths, not file paths).
pub fn extract_raw_imports(source: &str, language: Language) -> Vec<String> {
    match language {
        Language::Rust => extract_rust_imports(source),
        Language::Go => extract_go_imports(source),
        Language::Python => extract_python_imports(source),
        Language::Java => extract_java_imports(source),
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact => extract_ecmascript_imports(source),
    }
}

// --- Language-specific extractors ---

fn extract_rust_imports(source: &str) -> Vec<String> {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let mut paths = Vec::new();
    for item in &file.items {
        if let syn::Item::Use(use_item) = item {
            let raw = flatten_use_tree(&use_item.tree, "");
            paths.extend(raw);
        }
    }

    dedup(paths)
}

/// Recursively flatten a syn UseTree into full `crate::a::b` strings.
fn flatten_use_tree(tree: &syn::UseTree, prefix: &str) -> Vec<String> {
    match tree {
        syn::UseTree::Path(p) => {
            let seg = p.ident.to_string();
            let new_prefix = if prefix.is_empty() {
                seg
            } else {
                format!("{}::{}", prefix, seg)
            };
            flatten_use_tree(&p.tree, &new_prefix)
        }
        syn::UseTree::Name(n) => {
            let seg = n.ident.to_string();
            let full = if prefix.is_empty() {
                seg
            } else {
                format!("{}::{}", prefix, seg)
            };
            vec![full]
        }
        syn::UseTree::Rename(r) => {
            let seg = r.ident.to_string();
            let full = if prefix.is_empty() {
                seg
            } else {
                format!("{}::{}", prefix, seg)
            };
            vec![full]
        }
        syn::UseTree::Glob(_) => {
            // `use crate::foo::*;` — the prefix IS the module
            if prefix.is_empty() {
                vec![]
            } else {
                vec![prefix.to_string()]
            }
        }
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .flat_map(|item| flatten_use_tree(item, prefix))
            .collect(),
    }
}

fn extract_go_imports(source: &str) -> Vec<String> {
    use regex::Regex;

    // Block form: import ( ... )
    static BLOCK_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let block_re = BLOCK_RE.get_or_init(|| Regex::new(r#"import\s*\(([^)]*)\)"#).unwrap());

    // Path inside block or single import
    static PATH_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let path_re = PATH_RE.get_or_init(|| Regex::new(r#"(?:\w+\s+)?"([^"]+)""#).unwrap());

    let mut imports = Vec::new();

    // Extract block imports
    let mut block_ranges = Vec::new();
    for cap in block_re.captures_iter(source) {
        let m = cap.get(0).unwrap();
        block_ranges.push(m.range());
        let block = &cap[1];
        for pc in path_re.captures_iter(block) {
            imports.push(pc[1].to_string());
        }
    }

    // Single-line imports outside blocks
    static SINGLE_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let single_re =
        SINGLE_RE.get_or_init(|| Regex::new(r#"(?m)^import\s+(?:\w+\s+)?"([^"]+)""#).unwrap());
    for cap in single_re.captures_iter(source) {
        let m = cap.get(0).unwrap();
        // Skip if inside a block import we already processed
        if block_ranges.iter().any(|r| r.contains(&m.start())) {
            continue;
        }
        imports.push(cap[1].to_string());
    }

    dedup(imports)
}

fn extract_python_imports(source: &str) -> Vec<String> {
    use regex::Regex;

    static IMPORT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let import_re = IMPORT_RE.get_or_init(|| Regex::new(r"(?m)^\s*import\s+(\S+)").unwrap());

    static FROM_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let from_re = FROM_RE.get_or_init(|| Regex::new(r"(?m)^\s*from\s+(\S+)\s+import").unwrap());

    let mut imports = Vec::new();

    for cap in import_re.captures_iter(source) {
        imports.push(cap[1].trim_end_matches(',').to_string());
    }
    for cap in from_re.captures_iter(source) {
        imports.push(cap[1].to_string());
    }

    dedup(imports)
}

fn extract_java_imports(source: &str) -> Vec<String> {
    use regex::Regex;

    static IMPORT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let import_re = IMPORT_RE.get_or_init(|| Regex::new(r"(?m)^\s*import\s+([\w.]+)\s*;").unwrap());

    let imports: Vec<_> = import_re
        .captures_iter(source)
        .map(|c| c[1].to_string())
        .collect();
    dedup(imports)
}

fn extract_ecmascript_imports(source: &str) -> Vec<String> {
    use regex::Regex;

    static FROM_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let from_re =
        FROM_RE.get_or_init(|| Regex::new(r#"(?:import|from)\s+['"]([^'"]+)['"]"#).unwrap());

    static REQUIRE_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let require_re =
        REQUIRE_RE.get_or_init(|| Regex::new(r#"require\(['"]([^'"]+)['"]\)"#).unwrap());

    let mut imports = Vec::new();
    for cap in from_re.captures_iter(source) {
        imports.push(cap[1].to_string());
    }
    for cap in require_re.captures_iter(source) {
        imports.push(cap[1].to_string());
    }

    dedup(imports)
}

// --- Resolution helpers ---

/// Find crate root (dir containing Cargo.toml) by walking up from a file.
fn find_crate_root(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent()?;
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p,
            _ => return None,
        }
    }
}

/// Extract `[package] name` from Cargo.toml text.
fn extract_cargo_package_name(toml: &str) -> Option<String> {
    use regex::Regex;
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"(?m)^\s*name\s*=\s*"([^"]+)""#).unwrap());
    re.captures(toml).map(|c| c[1].to_string())
}

/// Build a map of `crate_name → src_dir` by scanning Cargo.toml files for all project files.
fn build_crate_map(all_files: &[&str], repo_root: &Path) -> HashMap<String, PathBuf> {
    let mut cargo_dirs: HashSet<PathBuf> = HashSet::new();

    for &file in all_files {
        let path = if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            repo_root.join(file)
        };
        let mut dir = match path.parent() {
            Some(d) => d.to_path_buf(),
            None => continue,
        };
        loop {
            if dir.join("Cargo.toml").exists() {
                cargo_dirs.insert(dir.clone());
                break;
            }
            match dir.parent() {
                Some(p) if p != dir.as_path() => dir = p.to_path_buf(),
                _ => break,
            }
        }
    }

    let mut map = HashMap::new();
    for dir in cargo_dirs {
        let cargo_toml = dir.join("Cargo.toml");
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            if let Some(name) = extract_cargo_package_name(&content) {
                let crate_name = name.replace('-', "_");
                let src_dir = dir.join("src");
                map.insert(crate_name, src_dir);
            }
        }
    }

    map
}

/// Attempt to resolve a raw import string to a project file path.
///
/// Returns `None` for external (library) imports or when resolution fails.
fn resolve_import(
    raw: &str,
    importing_file: &str,
    all_files_set: &HashSet<String>,
    language: Language,
    repo_root: &Path,
    crate_map: &HashMap<String, PathBuf>,
) -> Option<String> {
    match language {
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact => {
            resolve_ecmascript(raw, importing_file, all_files_set, repo_root)
        }
        Language::Rust => resolve_rust(raw, importing_file, all_files_set, repo_root, crate_map),
        Language::Go => resolve_go(raw, all_files_set),
        Language::Python => resolve_python(raw, importing_file, all_files_set, repo_root),
        Language::Java => resolve_java(raw, all_files_set),
    }
}

fn resolve_ecmascript(
    raw: &str,
    importing_file: &str,
    all_files_set: &HashSet<String>,
    repo_root: &Path,
) -> Option<String> {
    if !raw.starts_with("./") && !raw.starts_with("../") {
        return None; // external package
    }

    let importing_path = Path::new(importing_file);
    let parent = importing_path.parent().unwrap_or(Path::new("."));
    let abs_parent = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        repo_root.join(parent)
    };

    let base = abs_parent.join(raw);
    // Normalize without requiring the path to exist (canonicalize would fail)
    let base = normalize_path_lexically(&base);

    let exts = ["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];

    // Try with extensions
    for ext in &exts {
        let candidate = base.with_extension(ext);
        if let Some(hit) = check_in_set(&candidate, all_files_set, repo_root) {
            return Some(hit);
        }
    }

    // Try as directory/index.{ext}
    for ext in &["ts", "js", "tsx", "jsx"] {
        let candidate = base.join(format!("index.{}", ext));
        if let Some(hit) = check_in_set(&candidate, all_files_set, repo_root) {
            return Some(hit);
        }
    }

    None
}

fn resolve_rust(
    raw: &str,
    importing_file: &str,
    all_files_set: &HashSet<String>,
    repo_root: &Path,
    crate_map: &HashMap<String, PathBuf>,
) -> Option<String> {
    // Collect candidate module paths to try (from most-specific to least)
    let module_paths = rust_module_paths(raw);

    let importing_path = Path::new(importing_file);
    let abs_file = if importing_path.is_absolute() {
        importing_path.to_path_buf()
    } else {
        repo_root.join(importing_path)
    };

    for module_path in module_paths {
        if let Some(hit) =
            try_resolve_rust_module(&module_path, &abs_file, all_files_set, repo_root, crate_map)
        {
            return Some(hit);
        }
    }
    None
}

/// Extract candidate module path strings from a raw Rust use path.
///
/// Tries progressively shorter paths by stripping trailing segments.
/// e.g. `crate::foo::Bar` → [`crate::foo::Bar`, `crate::foo`]
fn rust_module_paths(raw: &str) -> Vec<String> {
    let mut paths = vec![raw.to_string()];
    let mut current = raw.to_string();
    while let Some(pos) = current.rfind("::") {
        current = current[..pos].to_string();
        paths.push(current.clone());
    }
    paths
}

/// Try to resolve a single Rust module path (e.g. `crate::foo`) to a file.
fn try_resolve_rust_module(
    module_path: &str,
    importing_abs: &Path,
    all_files_set: &HashSet<String>,
    repo_root: &Path,
    crate_map: &HashMap<String, PathBuf>,
) -> Option<String> {
    if let Some(sub) = module_path.strip_prefix("crate::") {
        let rust_path = sub.replace("::", "/");
        let crate_root = find_crate_root(importing_abs)?;
        let src_dir = crate_root.join("src");
        return try_rs_candidates(&src_dir, &rust_path, all_files_set, repo_root);
    }

    if let Some(sub) = module_path.strip_prefix("super::") {
        let rust_path = sub.replace("::", "/");
        let parent_dir = importing_abs.parent()?.parent()?;
        return try_rs_candidates(parent_dir, &rust_path, all_files_set, repo_root);
    }

    // cross-crate: first segment is crate name
    if let Some(sep) = module_path.find("::") {
        let crate_name = &module_path[..sep];
        let sub = &module_path[sep + 2..];
        let rust_path = sub.replace("::", "/");
        if let Some(src_dir) = crate_map.get(crate_name) {
            return try_rs_candidates(src_dir, &rust_path, all_files_set, repo_root);
        }
    }

    None
}

/// Try `{src_dir}/{path}.rs` and `{src_dir}/{path}/mod.rs`.
fn try_rs_candidates(
    src_dir: &Path,
    path: &str,
    all_files_set: &HashSet<String>,
    repo_root: &Path,
) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let candidates = [
        src_dir.join(format!("{}.rs", path)),
        src_dir.join(format!("{}/mod.rs", path)),
    ];
    for c in &candidates {
        if let Some(hit) = check_in_set(c, all_files_set, repo_root) {
            return Some(hit);
        }
    }
    None
}

fn resolve_go(raw: &str, all_files_set: &HashSet<String>) -> Option<String> {
    // last path segment = package directory name
    let last_seg = raw.split('/').next_back()?;
    if last_seg.is_empty() {
        return None;
    }

    // Find the first .go file whose parent directory name matches
    let mut candidate: Option<&str> = None;
    for file in all_files_set {
        if !file.ends_with(".go") {
            continue;
        }
        let parent_name = Path::new(file)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        if parent_name == Some(last_seg) {
            // Prefer earlier (lexically smaller) for determinism
            match candidate {
                None => candidate = Some(file.as_str()),
                Some(prev) if file.as_str() < prev => candidate = Some(file.as_str()),
                _ => {}
            }
        }
    }
    candidate.map(|s| s.to_string())
}

fn resolve_python(
    raw: &str,
    importing_file: &str,
    all_files_set: &HashSet<String>,
    repo_root: &Path,
) -> Option<String> {
    let importing_path = Path::new(importing_file);
    let parent = importing_path.parent().unwrap_or(Path::new("."));
    let abs_parent = if parent.is_absolute() {
        parent.to_path_buf()
    } else {
        repo_root.join(parent)
    };

    // Count leading dots (relative imports)
    let dots = raw.chars().take_while(|c| *c == '.').count();
    let module_name = &raw[dots..];

    let base_dir = if dots > 0 {
        let mut dir = abs_parent.clone();
        for _ in 1..dots {
            if let Some(p) = dir.parent() {
                dir = p.to_path_buf();
            }
        }
        dir
    } else {
        abs_parent.clone()
    };

    let module_path = module_name.replace('.', "/");

    let candidates: Vec<PathBuf> = if module_name.is_empty() {
        vec![base_dir.join("__init__.py")]
    } else {
        vec![
            base_dir.join(format!("{}.py", module_path)),
            base_dir.join(format!("{}/__init__.py", module_path)),
            repo_root.join(format!("{}.py", module_path)),
        ]
    };

    for c in candidates {
        if let Some(hit) = check_in_set(&c, all_files_set, repo_root) {
            return Some(hit);
        }
    }
    None
}

fn resolve_java(raw: &str, all_files_set: &HashSet<String>) -> Option<String> {
    // Drop well-known stdlib/framework prefixes
    let stdlib = [
        "java.",
        "javax.",
        "org.junit.",
        "org.springframework.",
        "com.google.",
        "android.",
        "kotlin.",
        "scala.",
    ];
    if stdlib.iter().any(|p| raw.starts_with(p)) {
        return None;
    }

    let java_file = format!("{}.java", raw.replace('.', "/"));
    for file in all_files_set {
        if file.ends_with(&java_file) {
            return Some(file.clone());
        }
    }
    None
}

// --- Utilities ---

/// Check if `candidate` (absolute path) is in `all_files_set`,
/// trying both the absolute form and the repo-root-relative form.
fn check_in_set(
    candidate: &Path,
    all_files_set: &HashSet<String>,
    repo_root: &Path,
) -> Option<String> {
    let abs_str = candidate.to_string_lossy().replace('\\', "/");
    if all_files_set.contains(&abs_str) {
        return Some(abs_str);
    }
    if let Ok(rel) = candidate.strip_prefix(repo_root) {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if all_files_set.contains(&rel_str) {
            return Some(rel_str);
        }
    }
    None
}

/// Lexically normalize a path (resolve `..` and `.` without hitting the filesystem).
fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                if matches!(components.last(), Some(std::path::Component::Normal(_))) {
                    components.pop();
                } else {
                    components.push(c);
                }
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Deduplicate a Vec<String> preserving order.
fn dedup(mut v: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    v.retain(|s| seen.insert(s.clone()));
    v
}

/// Resolve file-level import edges for a set of project source files.
///
/// Returns `(from_file, to_file)` pairs where both files are in the project.
/// External / unresolvable imports produce no edge.
pub fn resolve_file_deps(source_files: &[&str], repo_root: &Path) -> Vec<(String, String)> {
    let all_files_set: HashSet<String> = source_files.iter().map(|s| s.to_string()).collect();
    let crate_map = build_crate_map(source_files, repo_root);

    let mut edges = Vec::new();
    let mut seen_edges: HashSet<(String, String)> = HashSet::new();

    for &file in source_files {
        let lang = match Language::from_path(Path::new(file)) {
            Some(l) => l,
            None => continue,
        };

        let abs_path = if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            repo_root.join(file)
        };

        let source = match std::fs::read_to_string(&abs_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let raw_imports = extract_raw_imports(&source, lang);

        for raw in raw_imports {
            if let Some(to_file) =
                resolve_import(&raw, file, &all_files_set, lang, repo_root, &crate_map)
            {
                if to_file == file {
                    continue; // skip self-edges
                }
                let edge = (file.to_string(), to_file);
                if seen_edges.insert(edge.clone()) {
                    edges.push(edge);
                }
            }
        }
    }

    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ecmascript_imports() {
        let src = r#"
import React from 'react';
import { foo } from './utils';
import type { Bar } from '../types';
const x = require('./helper');
"#;
        let imports = extract_ecmascript_imports(src);
        assert!(imports.contains(&"react".to_string()));
        assert!(imports.contains(&"./utils".to_string()));
        assert!(imports.contains(&"../types".to_string()));
        assert!(imports.contains(&"./helper".to_string()));
    }

    #[test]
    fn test_extract_python_imports() {
        let src = "import os\nfrom utils import foo\nfrom .helper import bar\n";
        let imports = extract_python_imports(src);
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"utils".to_string()));
        assert!(imports.contains(&".helper".to_string()));
    }

    #[test]
    fn test_extract_java_imports() {
        let src = "import java.util.List;\nimport com.example.Foo;\n";
        let imports = extract_java_imports(src);
        assert!(imports.contains(&"java.util.List".to_string()));
        assert!(imports.contains(&"com.example.Foo".to_string()));
    }

    #[test]
    fn test_extract_go_imports_block() {
        let src = r#"
import (
    "fmt"
    "github.com/user/project/pkg/utils"
)
"#;
        let imports = extract_go_imports(src);
        assert!(imports.contains(&"fmt".to_string()));
        assert!(imports.contains(&"github.com/user/project/pkg/utils".to_string()));
    }

    #[test]
    fn test_extract_rust_imports() {
        let src = r#"
use crate::aggregates::SnapshotAggregates;
use crate::git;
use std::collections::HashMap;
"#;
        let imports = extract_rust_imports(src);
        assert!(imports.iter().any(|s| s.contains("aggregates")));
        assert!(imports.iter().any(|s| s.contains("git")));
    }

    #[test]
    fn test_flatten_use_tree_glob() {
        let src = "use crate::foo::*;";
        let imports = extract_rust_imports(src);
        // Should get `crate::foo` (the glob target)
        assert!(imports
            .iter()
            .any(|s| s == "crate::foo::*" || s == "crate::foo"));
    }

    #[test]
    fn test_resolve_java_drops_stdlib() {
        let set: HashSet<String> = HashSet::new();
        assert!(resolve_java("java.util.List", &set).is_none());
        assert!(resolve_java("javax.servlet.Servlet", &set).is_none());
    }

    #[test]
    fn test_normalize_path_lexically() {
        let p = Path::new("/foo/bar/../baz/./qux");
        let normalized = normalize_path_lexically(p);
        assert_eq!(normalized, Path::new("/foo/baz/qux"));
    }
}
