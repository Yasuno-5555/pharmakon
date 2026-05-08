//! BM25-powered tool meta catalog for deferred loading.
//!
//! This module lives in `pharmakon-common` because the `ToolMetaCatalog` struct
//! depends only on types already defined here (ToolMeta, ToolCategory, ExecutionProfile).
//! The actual catalog population (`build_default_catalog()`) lives in `pharmakon-tools`.
//! to avoid circular dependencies.

use crate::{SideEffectLevel, ToolCategory, ToolMeta};
use std::collections::HashMap;

/// Stop words filtered from BM25 indexing.
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "to", "of", "in", "for",
    "on", "with", "at", "by", "from", "as", "into", "through", "during",
    "before", "after", "above", "below", "between", "under", "and", "but",
    "or", "nor", "not", "so", "yet", "both", "either", "neither", "each",
    "this", "that", "these", "those", "it", "its", "use", "using", "used",
    "tool", "tools", "based", "specific", "given",
];

/// BM25-indexed tool catalog for deferred loading.
pub struct ToolMetaCatalog {
    entries: Vec<ToolMeta>,
    inverted_index: HashMap<String, Vec<(usize, f32)>>,
    avg_doc_len: f32,
    doc_lengths: Vec<usize>,
}

/// A scored search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub meta: ToolMeta,
    pub score: f32,
}

impl ToolMetaCatalog {
    /// Build a catalog from a list of ToolMeta entries.
    pub fn new(entries: Vec<ToolMeta>) -> Self {
        let mut inverted_index: HashMap<String, Vec<(usize, f32)>> = HashMap::new();
        let mut doc_lengths = Vec::with_capacity(entries.len());

        for (idx, meta) in entries.iter().enumerate() {
            let doc_text = format!(
                "{} {} {}",
                meta.name,
                meta.description,
                meta.category.as_str()
            );
            let tokens = tokenize(&doc_text);
            doc_lengths.push(tokens.len());

            let mut tf_map: HashMap<String, f32> = HashMap::new();
            for token in &tokens {
                *tf_map.entry(token.clone()).or_insert(0.0) += 1.0;
            }

            for (token, tf) in tf_map {
                inverted_index.entry(token).or_default().push((idx, tf));
            }
        }

        let total_len: usize = doc_lengths.iter().sum();
        let avg_doc_len = if entries.is_empty() {
            1.0
        } else {
            total_len as f32 / entries.len() as f32
        };

        Self {
            entries,
            inverted_index,
            avg_doc_len,
            doc_lengths,
        }
    }

    /// Search the catalog using BM25 scoring.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        let query_tokens = tokenize(query);
        let n = self.entries.len() as f32;
        let k1: f32 = 1.2;
        let b: f32 = 0.75;

        let mut scores: Vec<f32> = vec![0.0; self.entries.len()];

        for token in &query_tokens {
            if let Some(postings) = self.inverted_index.get(token) {
                let df = postings.len() as f32;
                let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

                for &(idx, tf) in postings {
                    let dl = self.doc_lengths[idx] as f32;
                    let tf_norm =
                        (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / self.avg_doc_len)));
                    scores[idx] += idf * tf_norm;
                }
            }
        }

        // Boost exact name matches
        let query_lower = query.to_lowercase();
        for (idx, meta) in self.entries.iter().enumerate() {
            if query_lower.contains(&meta.name.to_lowercase()) {
                scores[idx] *= 2.0;
            }
        }

        let mut results: Vec<SearchResult> = scores
            .iter()
            .enumerate()
            .filter(|(_, s)| **s > 0.0)
            .map(|(idx, &score)| SearchResult {
                meta: self.entries[idx].clone(),
                score,
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    /// Get all tools in a specific category.
    pub fn by_category(&self, category: &ToolCategory) -> Vec<&ToolMeta> {
        self.entries
            .iter()
            .filter(|m| &m.category == category)
            .collect()
    }

    /// Get a specific tool's metadata by name.
    pub fn get(&self, name: &str) -> Option<&ToolMeta> {
        self.entries.iter().find(|m| m.name == name)
    }

    pub fn all(&self) -> &[ToolMeta] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Generate a compact summary for system prompt injection.
    pub fn catalog_summary(&self) -> String {
        let mut out = String::from("Available tool categories:\n");
        let mut by_cat: HashMap<String, Vec<&ToolMeta>> = HashMap::new();
        for meta in &self.entries {
            by_cat
                .entry(meta.category.as_str().to_string())
                .or_default()
                .push(meta);
        }

        for (cat, tools) in &by_cat {
            out.push_str(&format!("\n[{}]:\n", cat));
            for tool in tools {
                let side = match tool.profile.side_effect_level {
                    SideEffectLevel::None => "pure",
                    SideEffectLevel::Local => "local",
                    SideEffectLevel::Irreversible => "irreversible",
                };
                out.push_str(&format!(
                    "  - {}: {} [{}]\n",
                    tool.name,
                    truncate_desc(&tool.description, 60),
                    side,
                ));
            }
        }
        out
    }

    /// Generate a capability-based catalog summary.
    /// Groups 65+ tools into 10 semantic capabilities for ultra-compact prompt injection.
    /// Token savings: ~90% reduction (~200 tokens vs ~2000 for the full catalog).
    pub fn capability_summary(&self) -> String {
        crate::capability::capability_catalog_summary()
    }

    /// Get all tools mapped to a specific capability.
    pub fn by_capability(&self, capability: &crate::capability::Capability) -> Vec<&ToolMeta> {
        self.entries.iter().filter(|m| crate::capability::Capability::from_tool_name(&m.name) == Some(*capability)).collect()
    }
}

/// Tokenize text for BM25 indexing.
pub fn tokenize(text: &str) -> Vec<String> {
    let stop_set: std::collections::HashSet<&str> = STOP_WORDS.iter().copied().collect();

    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && s.len() > 1)
        .filter(|s| !stop_set.contains(s))
        .map(|s| s.to_string())
        .collect()
}

fn truncate_desc(desc: &str, max_len: usize) -> String {
    if desc.len() <= max_len {
        desc.to_string()
    } else {
        format!("{}…", &desc[..max_len])
    }
}