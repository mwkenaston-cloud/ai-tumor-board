//! AI-attribution metrics over a physician note, mirroring
//! src/services/noteBlocks.ts so `.atbr` responses carry the same breakdown the
//! prototype's buildOutputJson() produced. Kept pure and tested.
//!
//! Consumed by response import (coordinator side); exercised now by unit tests.
#![allow(dead_code)]

use serde::Serialize;

use super::models::NoteBlock;

/// Character-level Levenshtein distance.
pub fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Similarity 0–100 between original AI text and its edited form.
pub fn similarity_percent(original: &str, edited: &str) -> f64 {
    if original.is_empty() && edited.is_empty() {
        return 100.0;
    }
    let dist = levenshtein(original, edited) as f64;
    let max_len = original.chars().count().max(edited.chars().count()).max(1) as f64;
    ((1.0 - dist / max_len) * 100.0).round()
}

fn is_ai_edited(b: &NoteBlock) -> bool {
    match &b.original_text {
        Some(orig) => b.current_text.trim() != orig.trim(),
        None => false,
    }
}

/// Deterministic plain-text note: non-empty trimmed blocks joined by a blank line.
pub fn derive_note_text(blocks: &[NoteBlock]) -> String {
    blocks
        .iter()
        .map(|b| b.current_text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttributionMetrics {
    pub word_count: usize,
    pub char_count: usize,
    pub chars_from_llm_unmodified: usize,
    pub chars_from_llm_edited: usize,
    pub chars_typed_by_physician: usize,
    pub pct_ai_unmodified: u32,
    pub pct_ai_edited: u32,
    pub pct_physician_original: u32,
    pub pct_derived_from_llm: u32,
}

pub fn attribution_metrics(blocks: &[NoteBlock]) -> AttributionMetrics {
    let mut unmodified = 0usize;
    let mut edited = 0usize;
    let mut physician = 0usize;

    for b in blocks {
        let len = b.current_text.trim().chars().count();
        if len == 0 {
            continue;
        }
        if b.block_type == "user" {
            physician += len;
        } else if is_ai_edited(b) {
            edited += len;
        } else {
            unmodified += len;
        }
    }

    let total = (unmodified + edited + physician).max(1);
    let text = derive_note_text(blocks);
    let words = text.split_whitespace().count();
    let pct = |n: usize| ((n as f64 / total as f64) * 100.0).round() as u32;

    AttributionMetrics {
        word_count: words,
        char_count: text.chars().count(),
        chars_from_llm_unmodified: unmodified,
        chars_from_llm_edited: edited,
        chars_typed_by_physician: physician,
        pct_ai_unmodified: pct(unmodified),
        pct_ai_edited: pct(edited),
        pct_physician_original: pct(physician),
        pct_derived_from_llm: pct(unmodified + edited),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> NoteBlock {
        NoteBlock { id: "u".into(), block_type: "user".into(), recommendation_id: None, original_text: None, current_text: text.into(), position: 0 }
    }
    fn ai(orig: &str, curr: &str) -> NoteBlock {
        NoteBlock { id: "a".into(), block_type: "ai".into(), recommendation_id: Some("R".into()), original_text: Some(orig.into()), current_text: curr.into(), position: 1 }
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("kitten", "kitten"), 0);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn similarity_scales() {
        assert_eq!(similarity_percent("hello", "hello"), 100.0);
        assert!(similarity_percent("hello world", "hello there") < 100.0);
    }

    #[test]
    fn attribution_splits_sources() {
        // 10 physician chars, 10 unmodified AI chars.
        let blocks = vec![user("0123456789"), ai("ABCDEFGHIJ", "ABCDEFGHIJ")];
        let m = attribution_metrics(&blocks);
        assert_eq!(m.chars_typed_by_physician, 10);
        assert_eq!(m.chars_from_llm_unmodified, 10);
        assert_eq!(m.pct_physician_original, 50);
        assert_eq!(m.pct_ai_unmodified, 50);
        assert_eq!(m.pct_derived_from_llm, 50);
    }

    #[test]
    fn edited_ai_block_counts_as_edited() {
        let blocks = vec![ai("original text", "original text edited")];
        let m = attribution_metrics(&blocks);
        assert!(m.chars_from_llm_edited > 0);
        assert_eq!(m.chars_from_llm_unmodified, 0);
    }
}
