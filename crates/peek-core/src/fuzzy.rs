use nucleo::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo::Matcher;
use nucleo::Utf32Str;

pub struct FuzzyMatch {
    pub index: usize,
    pub score: u32,
}

/// Fuzzy match a query against a list of candidates.
/// Returns indices and scores of matching candidates, sorted by score descending.
pub fn fuzzy_match(query: &str, candidates: &[&str]) -> Vec<FuzzyMatch> {
    if query.is_empty() {
        return candidates
            .iter()
            .enumerate()
            .map(|(i, _)| FuzzyMatch { index: i, score: 0 })
            .collect();
    }

    let pattern = Pattern::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::default();
    let mut buf = Vec::new();
    let mut matches: Vec<FuzzyMatch> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, candidate)| {
            let haystack = Utf32Str::new(candidate, &mut buf);
            pattern.score(haystack, &mut matcher).map(|score| FuzzyMatch {
                index: i,
                score,
            })
        })
        .collect();

    matches.sort_by(|a, b| b.score.cmp(&a.score));
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_returns_all() {
        let candidates = vec!["dev", "build", "test"];
        let results = fuzzy_match("", &candidates);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_exact_match() {
        let candidates = vec!["dev", "build", "test"];
        let results = fuzzy_match("dev", &candidates);
        assert!(!results.is_empty());
        assert_eq!(results[0].index, 0);
    }

    #[test]
    fn test_fuzzy_match_subsequence() {
        let candidates = vec!["dev", "development", "docker"];
        let results = fuzzy_match("dv", &candidates);
        assert!(!results.is_empty());
        assert!(results.iter().any(|m| m.index == 0));
    }

    #[test]
    fn test_no_match() {
        let candidates = vec!["dev", "build", "test"];
        let results = fuzzy_match("xyz", &candidates);
        assert!(results.is_empty());
    }
}
