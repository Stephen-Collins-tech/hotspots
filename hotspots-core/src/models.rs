//! Model risk map extraction and aggregation.
//!
//! This module keeps the first model-map implementation structural: it extracts
//! first-party model declarations, associates functions by same-file and direct
//! import edges, then ranks models by concentrated risk.

use crate::language::Language;
use crate::risk::RiskBand;
use crate::snapshot::{FunctionSnapshot, Snapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

const DEFAULT_FUNCTIONS_PER_MODEL: usize = 5;
const MIN_ASSOCIATED_FUNCTIONS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ModelDecl {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub language: Language,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ModelRiskMap {
    pub models: Vec<ModelRiskEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ModelRiskLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ModelRiskEntry {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub language: Language,
    pub kind: String,
    pub score: f64,
    pub critical: usize,
    pub high: usize,
    pub moderate: usize,
    pub low: usize,
    pub functions: Vec<ModelFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ModelFunction {
    pub function_id: String,
    pub function: String,
    pub file: String,
    pub line: u32,
    pub lrs: f64,
    pub activity_risk: Option<f64>,
    pub band: RiskBand,
    pub quadrant: Option<String>,
    pub association: AssociationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ModelRiskLink {
    pub source: usize,
    pub target: usize,
    pub shared_functions: usize,
    pub shared_risk: f64,
    pub functions: Vec<String>,
}

struct ModelRiskBuildEntry {
    entry: ModelRiskEntry,
    associated: Vec<ModelFunction>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AssociationKind {
    SameFile,
    DirectImport,
}

pub fn compute_model_risk_map(
    source_root: &Path,
    repo_root: &Path,
    snapshot: &Snapshot,
    top_models: Option<usize>,
) -> Result<ModelRiskMap> {
    let models = extract_models(source_root, repo_root)?;
    let model_files: BTreeSet<String> = models.iter().map(|m| m.file.clone()).collect();
    let mut model_counts_by_file: HashMap<String, usize> = HashMap::new();
    for model in &models {
        *model_counts_by_file.entry(model.file.clone()).or_default() += 1;
    }
    if models.is_empty() {
        return Ok(ModelRiskMap {
            models: Vec::new(),
            links: Vec::new(),
        });
    }

    let mut files: Vec<String> = snapshot.functions.iter().map(|f| f.file.clone()).collect();
    files.extend(model_files.iter().cloned());
    files.sort();
    files.dedup();
    let file_refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let import_edges = crate::imports::resolve_file_deps(&file_refs, repo_root);

    let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
    for (from, to) in import_edges {
        import_map.entry(from).or_default().insert(to);
    }
    let tokens_by_file = load_source_tokens(&files, repo_root);

    let mut functions_by_file: BTreeMap<&str, Vec<&FunctionSnapshot>> = BTreeMap::new();
    for function in &snapshot.functions {
        functions_by_file
            .entry(function.file.as_str())
            .or_default()
            .push(function);
    }

    let mut entries = Vec::new();
    for model in models {
        let mut associated = Vec::new();
        let mut seen = HashSet::new();

        if let Some(functions) = functions_by_file.get(model.file.as_str()) {
            let model_count = model_counts_by_file
                .get(&model.file)
                .copied()
                .unwrap_or_default();
            add_same_file_functions(&mut associated, &mut seen, functions, &model, model_count);
        }

        for (file, imports) in &import_map {
            if imports.contains(&model.file)
                && tokens_by_file
                    .get(file)
                    .is_some_and(|tokens| tokens.contains(&model.name))
            {
                add_associated_functions(
                    &mut associated,
                    &mut seen,
                    direct_import_functions(
                        functions_by_file
                            .get(file.as_str())
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        &model.name,
                    )
                    .iter(),
                    AssociationKind::DirectImport,
                );
            }
        }

        if associated.len() < MIN_ASSOCIATED_FUNCTIONS {
            continue;
        }

        associated.sort_by(compare_model_functions);
        let score = associated
            .iter()
            .take(DEFAULT_FUNCTIONS_PER_MODEL)
            .map(function_score)
            .sum();
        let (critical, high, moderate, low) = band_counts(&associated);
        let mut top_functions = associated.clone();
        top_functions.truncate(DEFAULT_FUNCTIONS_PER_MODEL);

        entries.push(ModelRiskBuildEntry {
            entry: ModelRiskEntry {
                name: model.name,
                file: model.file,
                line: model.line,
                language: model.language,
                kind: model.kind,
                score,
                critical,
                high,
                moderate,
                low,
                functions: top_functions,
            },
            associated,
        });
    }

    entries.sort_by(|a, b| {
        b.entry
            .score
            .partial_cmp(&a.entry.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entry.file.cmp(&b.entry.file))
            .then_with(|| a.entry.line.cmp(&b.entry.line))
            .then_with(|| a.entry.name.cmp(&b.entry.name))
    });
    if let Some(limit) = top_models {
        entries.truncate(limit);
    }

    let links = build_model_links(&entries);
    let models = entries.into_iter().map(|entry| entry.entry).collect();

    Ok(ModelRiskMap { models, links })
}

fn build_model_links(entries: &[ModelRiskBuildEntry]) -> Vec<ModelRiskLink> {
    let mut links = Vec::new();
    for source_idx in 0..entries.len() {
        let source_functions: HashMap<&str, &ModelFunction> = entries[source_idx]
            .associated
            .iter()
            .map(|function| (function.function_id.as_str(), function))
            .collect();
        for (target_idx, target_entry) in entries.iter().enumerate().skip(source_idx + 1) {
            let mut shared = Vec::new();
            let mut shared_risk = 0.0;
            for function in &target_entry.associated {
                if let Some(source_function) = source_functions.get(function.function_id.as_str()) {
                    shared_risk += function_score(function).max(function_score(source_function));
                    shared.push(function.function.clone());
                }
            }
            if !shared.is_empty() {
                let shared_functions = shared.len();
                shared.sort();
                shared.truncate(5);
                links.push(ModelRiskLink {
                    source: source_idx,
                    target: target_idx,
                    shared_functions,
                    shared_risk,
                    functions: shared,
                });
            }
        }
    }
    links.sort_by(|a, b| {
        b.shared_functions
            .cmp(&a.shared_functions)
            .then_with(|| {
                b.shared_risk
                    .partial_cmp(&a.shared_risk)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
    });
    links
}

fn load_source_tokens(files: &[String], repo_root: &Path) -> HashMap<String, HashSet<String>> {
    let mut tokens_by_file = HashMap::new();
    for file in files {
        let path = if Path::new(file).is_absolute() {
            std::path::PathBuf::from(file)
        } else {
            repo_root.join(file)
        };
        if let Ok(source) = std::fs::read_to_string(path) {
            tokens_by_file.insert(file.clone(), source_tokens(&source));
        }
    }
    tokens_by_file
}

pub fn extract_models(source_root: &Path, repo_root: &Path) -> Result<Vec<ModelDecl>> {
    let source_files = crate::collect_source_files(source_root)?;
    let mut models = Vec::new();
    for path in source_files {
        let language = match Language::from_path(&path) {
            Some(language) => language,
            None => continue,
        };
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let file = normalize_file(&path, repo_root);
        models.extend(extract_models_from_source(&source, language, file));
    }
    models.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(models)
}

fn add_associated_functions<'a>(
    out: &mut Vec<ModelFunction>,
    seen: &mut HashSet<String>,
    functions: impl Iterator<Item = &'a &'a FunctionSnapshot>,
    association: AssociationKind,
) {
    for function in functions {
        if seen.insert(function.function_id.clone()) {
            out.push(ModelFunction {
                function_id: function.function_id.clone(),
                function: function_name(function),
                file: function.file.clone(),
                line: function.line,
                lrs: function.lrs,
                activity_risk: function.activity_risk,
                band: function.band,
                quadrant: function.quadrant.clone(),
                association,
            });
        }
    }
}

fn add_same_file_functions(
    out: &mut Vec<ModelFunction>,
    seen: &mut HashSet<String>,
    functions: &[&FunctionSnapshot],
    model: &ModelDecl,
    model_count_in_file: usize,
) {
    let method_prefix = format!("{}::", model.name);
    let methods: Vec<&FunctionSnapshot> = functions
        .iter()
        .copied()
        .filter(|function| function_name(function).starts_with(&method_prefix))
        .collect();
    if !methods.is_empty() {
        add_associated_functions(out, seen, methods.iter(), AssociationKind::SameFile);
        return;
    }

    let mentioned: Vec<&FunctionSnapshot> = functions
        .iter()
        .copied()
        .filter(|function| function_name_mentions_model(&function_name(function), &model.name))
        .collect();
    if !mentioned.is_empty() {
        add_associated_functions(out, seen, mentioned.iter(), AssociationKind::SameFile);
        return;
    }

    if model_count_in_file <= 1 {
        add_associated_functions(out, seen, functions.iter(), AssociationKind::SameFile);
    }
}

fn direct_import_functions<'a>(
    functions: &'a [&'a FunctionSnapshot],
    model_name: &str,
) -> Vec<&'a FunctionSnapshot> {
    let mentioned: Vec<&FunctionSnapshot> = functions
        .iter()
        .copied()
        .filter(|function| function_name_mentions_model(&function_name(function), model_name))
        .collect();
    if !mentioned.is_empty() || functions.len() > 1 {
        return mentioned;
    }
    functions.to_vec()
}

fn source_tokens(source: &str) -> HashSet<String> {
    source
        .split(|c: char| !(c == '_' || c == '$' || c.is_ascii_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn function_name_mentions_model(name: &str, model_name: &str) -> bool {
    name.to_lowercase().contains(&model_name.to_lowercase())
}

fn compare_model_functions(a: &ModelFunction, b: &ModelFunction) -> std::cmp::Ordering {
    function_score(b)
        .partial_cmp(&function_score(a))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.file.cmp(&b.file))
        .then_with(|| a.line.cmp(&b.line))
        .then_with(|| a.function.cmp(&b.function))
}

fn function_score(function: &ModelFunction) -> f64 {
    function.activity_risk.unwrap_or(function.lrs)
}

fn band_counts(functions: &[ModelFunction]) -> (usize, usize, usize, usize) {
    let mut critical = 0;
    let mut high = 0;
    let mut moderate = 0;
    let mut low = 0;
    for function in functions {
        match function.band {
            RiskBand::Critical => critical += 1,
            RiskBand::High => high += 1,
            RiskBand::Moderate => moderate += 1,
            RiskBand::Low => low += 1,
        }
    }
    (critical, high, moderate, low)
}

fn function_name(function: &FunctionSnapshot) -> String {
    function
        .function_id
        .strip_prefix(&format!("{}::", function.file))
        .unwrap_or(function.function_id.as_str())
        .to_string()
}

fn normalize_file(path: &Path, repo_root: &Path) -> String {
    if path.is_absolute() {
        path.to_string_lossy().replace('\\', "/")
    } else {
        repo_root.join(path).to_string_lossy().replace('\\', "/")
    }
}

fn extract_models_from_source(source: &str, language: Language, file: String) -> Vec<ModelDecl> {
    match language {
        Language::Rust => extract_regex_models(source, language, file, RUST_MODEL_PATTERNS),
        Language::Go => extract_regex_models(source, language, file, GO_MODEL_PATTERNS),
        Language::Python => extract_regex_models(source, language, file, PYTHON_MODEL_PATTERNS),
        Language::Java => extract_regex_models(source, language, file, JAVA_MODEL_PATTERNS),
        Language::TypeScript
        | Language::TypeScriptReact
        | Language::JavaScript
        | Language::JavaScriptReact
        | Language::Vue => extract_regex_models(source, language, file, ECMASCRIPT_MODEL_PATTERNS),
    }
}

struct PatternSpec {
    pattern: &'static str,
    kind: &'static str,
}

const ECMASCRIPT_MODEL_PATTERNS: &[PatternSpec] = &[
    PatternSpec {
        pattern: r"\b(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)",
        kind: "interface",
    },
    PatternSpec {
        pattern: r"\b(?:export\s+)?class\s+([A-Za-z_$][\w$]*)",
        kind: "class",
    },
    PatternSpec {
        pattern: r"\b(?:export\s+)?type\s+([A-Za-z_$][\w$]*)\s*=",
        kind: "type",
    },
];

const GO_MODEL_PATTERNS: &[PatternSpec] = &[PatternSpec {
    pattern: r"\btype\s+([A-Za-z_]\w*)\s+struct\b",
    kind: "struct",
}];

const RUST_MODEL_PATTERNS: &[PatternSpec] = &[
    PatternSpec {
        pattern: r"\b(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_]\w*)\b",
        kind: "struct",
    },
    PatternSpec {
        pattern: r"\b(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Za-z_]\w*)\b",
        kind: "enum",
    },
];

const PYTHON_MODEL_PATTERNS: &[PatternSpec] = &[PatternSpec {
    pattern: r"(?m)^\s*class\s+([A-Za-z_]\w*)\b",
    kind: "class",
}];

const JAVA_MODEL_PATTERNS: &[PatternSpec] = &[
    PatternSpec {
        pattern: r"(?m)^\s*(?:(?:public|private|protected|abstract|final|static)\s+)*class\s+([A-Za-z_]\w*)\b",
        kind: "class",
    },
    PatternSpec {
        pattern: r"(?m)^\s*(?:(?:public|private|protected|abstract|static)\s+)*interface\s+([A-Za-z_]\w*)\b",
        kind: "interface",
    },
    PatternSpec {
        pattern: r"(?m)^\s*(?:(?:public|private|protected|final|static)\s+)*record\s+([A-Za-z_]\w*)\b",
        kind: "record",
    },
];

fn extract_regex_models(
    source: &str,
    language: Language,
    file: String,
    specs: &[PatternSpec],
) -> Vec<ModelDecl> {
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    for spec in specs {
        let regex = match regex::Regex::new(spec.pattern) {
            Ok(regex) => regex,
            Err(_) => continue,
        };
        for captures in regex.captures_iter(source) {
            let Some(name_match) = captures.get(1) else {
                continue;
            };
            let name = name_match.as_str().to_string();
            let line = line_number(source, name_match.start());
            if seen.insert((name.clone(), line, spec.kind)) {
                models.push(ModelDecl {
                    name,
                    file: file.clone(),
                    line,
                    language,
                    kind: spec.kind.to_string(),
                });
            }
        }
    }
    models.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.name.cmp(&b.name)));
    models
}

fn line_number(source: &str, offset: usize) -> u32 {
    source[..offset].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

pub fn render_model_risk_text(map: &ModelRiskMap, top: Option<usize>) -> String {
    let mut output = String::new();
    output.push_str("Model Risk Map\n");
    output.push_str(&"=".repeat(80));
    output.push('\n');
    output.push('\n');

    let limit = top.unwrap_or(map.models.len());
    for (idx, model) in map.models.iter().take(limit).enumerate() {
        output.push_str(&format!(
            "#{:<3} {:<24} {:<32} criticalx{} highx{} moderatex{}\n",
            idx + 1,
            model.name,
            truncate_middle(&model.file, 32),
            model.critical,
            model.high,
            model.moderate
        ));
        for function in &model.functions {
            let quadrant = function.quadrant.as_deref().unwrap_or("-");
            output.push_str(&format!(
                "     {:<28} {:<32} LRS {:>6.2} [{}]\n",
                truncate_middle(&function.function, 28),
                format!("{}:{}", truncate_middle(&function.file, 24), function.line),
                function.lrs,
                quadrant
            ));
        }
        output.push('\n');
    }

    output.push_str("Models ranked by: sum of top-5 associated function risk scores\n");
    output
}

pub fn render_model_risk_json(map: &ModelRiskMap) -> Result<String> {
    serde_json::to_string_pretty(map).context("failed to render model risk JSON")
}

fn truncate_middle(value: &str, width: usize) -> String {
    if value.len() <= width {
        return format!("{value:<width$}");
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    format!("{}...", &value[..width - 3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_model_declarations_from_supported_languages() {
        let ts = "export interface Order {}\ntype User = { id: string }\nclass Cart {}";
        let models = extract_models_from_source(ts, Language::TypeScript, "a.ts".to_string());
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].name, "Order");
        assert_eq!(models[1].name, "User");
        assert_eq!(models[2].name, "Cart");

        let rust = "struct Order { id: u64 }\nenum State { Ready(String), Done }";
        let models = extract_models_from_source(rust, Language::Rust, "a.rs".to_string());
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].kind, "struct");
        assert_eq!(models[1].kind, "enum");
    }
}
