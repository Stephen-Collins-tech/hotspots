//! Deterministic phrase table for the `--explain` output layer.
//!
//! Reads per-feature percentile ranks within the current repo and maps them to
//! human-readable phrases. No model internals, no LLM, no network.

/// Feature is considered elevated when it is at or above this percentile within the repo.
pub const ELEVATED: f32 = 0.80;

/// Per-feature percentile rank (0.0–1.0) within the repo being analyzed.
#[derive(Debug, Clone)]
pub struct FeaturePercentiles {
    pub lrs: f32,
    pub cc: f32,
    pub nd: f32,
    pub loc: f32,
    pub fo: f32,
    pub fan_in: f32,
    pub total_churn: f32,
    pub authors_90d: f32,
    pub directed_coupling: f32,
}

/// Return a phrase string (sentence-cased, ending with `.`) describing the top `n`
/// elevated features. Checks co-occurrence pairs first; falls back to single-feature
/// phrases joined with commas. Returns an empty string when no feature is elevated.
pub fn top_phrases(features: &FeaturePercentiles, n: usize) -> String {
    let candidates: [(&str, f32); 7] = [
        ("total_churn", features.total_churn),
        ("lrs", features.lrs),
        ("fan_in", features.fan_in),
        ("authors_90d", features.authors_90d),
        ("directed_coupling", features.directed_coupling),
        ("cc", features.cc),
        ("nd", features.nd),
    ];

    let mut elevated: Vec<(&str, f32)> = candidates
        .iter()
        .filter(|(_, p)| *p >= ELEVATED)
        .copied()
        .collect();
    elevated.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if elevated.is_empty() {
        // No features at the threshold — fall back to the single highest feature.
        let mut all = candidates;
        all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        return capitalize_first(&format!("{}.", single_phrase(all[0].0)));
    }

    // Co-occurrence pair check on the top-2 elevated features.
    if elevated.len() >= 2 {
        if let Some(phrase) = pair_phrase(elevated[0].0, elevated[1].0) {
            return capitalize_first(&format!("{}.", phrase));
        }
    }

    // Single-feature fallback: join top n phrases.
    let phrases: Vec<&'static str> = elevated
        .iter()
        .take(n.max(1))
        .map(|(name, _)| single_phrase(name))
        .collect();
    join_phrases(&phrases)
}

fn single_phrase(name: &str) -> &'static str {
    match name {
        "total_churn" => "high lifetime churn",
        "lrs" => "structurally complex",
        "fan_in" => "depended on by many callers",
        "authors_90d" => "no clear owner",
        "directed_coupling" => "tightly coupled to other hotspots",
        "cc" => "high cyclomatic complexity",
        "nd" => "deeply nested",
        _ => "elevated risk signal",
    }
}

fn pair_phrase(a: &str, b: &str) -> Option<&'static str> {
    let pair = match (a, b) {
        ("total_churn", "fan_in") | ("fan_in", "total_churn") => {
            "churns heavily and is load-bearing"
        }
        ("total_churn", "authors_90d") | ("authors_90d", "total_churn") => {
            "high churn with no clear owner"
        }
        ("lrs", "fan_in") | ("fan_in", "lrs") => "structurally complex and widely depended on",
        ("lrs", "total_churn") | ("total_churn", "lrs") => "complex and frequently changed",
        ("authors_90d", "fan_in") | ("fan_in", "authors_90d") => {
            "no clear owner and called from many places"
        }
        ("directed_coupling", "total_churn") | ("total_churn", "directed_coupling") => {
            "coupled to hotspots and changing frequently"
        }
        _ => return None,
    };
    Some(pair)
}

fn join_phrases(phrases: &[&'static str]) -> String {
    match phrases.len() {
        0 => String::new(),
        1 => capitalize_first(&format!("{}.", phrases[0])),
        2 => capitalize_first(&format!("{} and {}.", phrases[0], phrases[1])),
        _ => {
            let head = phrases[..phrases.len() - 1].join(", ");
            capitalize_first(&format!("{}, and {}.", head, phrases[phrases.len() - 1]))
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_low() -> FeaturePercentiles {
        FeaturePercentiles {
            lrs: 0.0,
            cc: 0.0,
            nd: 0.0,
            loc: 0.0,
            fo: 0.0,
            fan_in: 0.0,
            total_churn: 0.0,
            authors_90d: 0.0,
            directed_coupling: 0.0,
        }
    }

    #[test]
    fn test_pair_phrase_churn_fanin() {
        let fp = FeaturePercentiles {
            total_churn: 0.95,
            fan_in: 0.90,
            ..all_low()
        };
        let result = top_phrases(&fp, 3);
        assert_eq!(result, "Churns heavily and is load-bearing.");
    }

    #[test]
    fn test_single_phrase_authors() {
        let fp = FeaturePercentiles {
            authors_90d: 0.92,
            ..all_low()
        };
        let result = top_phrases(&fp, 3);
        assert_eq!(result, "No clear owner.");
    }

    #[test]
    fn test_three_features_joined() {
        let fp = FeaturePercentiles {
            total_churn: 0.95,
            lrs: 0.92,
            fan_in: 0.91,
            ..all_low()
        };
        let result = top_phrases(&fp, 3);
        // top-2 are total_churn + lrs → pair "complex and frequently changed"
        assert_eq!(result, "Complex and frequently changed.");
    }

    #[test]
    fn test_fallback_when_no_pair() {
        let fp = FeaturePercentiles {
            cc: 0.95,
            nd: 0.92,
            ..all_low()
        };
        let result = top_phrases(&fp, 2);
        assert_eq!(result, "High cyclomatic complexity and deeply nested.");
    }

    #[test]
    fn test_no_elevated_falls_back_to_highest() {
        let fp = FeaturePercentiles {
            lrs: 0.50,
            ..all_low()
        };
        let result = top_phrases(&fp, 3);
        assert_eq!(result, "Structurally complex.");
    }

    #[test]
    fn test_deterministic() {
        let fp = FeaturePercentiles {
            total_churn: 0.95,
            fan_in: 0.90,
            ..all_low()
        };
        assert_eq!(top_phrases(&fp, 3), top_phrases(&fp, 3));
    }
}
