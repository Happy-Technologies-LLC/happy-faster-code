use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// BM25 keyword search index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25Index {
    /// Document ID -> tokenized terms
    documents: HashMap<String, Vec<String>>,
    /// Term -> document IDs containing it
    inverted_index: HashMap<String, Vec<String>>,
    /// Document ID -> document length
    doc_lengths: HashMap<String, usize>,
    /// Average document length
    avg_doc_len: f64,
    /// Total number of documents
    num_docs: usize,
    /// BM25 parameters
    k1: f64,
    b: f64,
}

impl BM25Index {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            inverted_index: HashMap::new(),
            doc_lengths: HashMap::new(),
            avg_doc_len: 0.0,
            num_docs: 0,
            k1: 1.5,
            b: 0.75,
        }
    }

    /// Add a document to the index.
    pub fn add_document(&mut self, doc_id: &str, text: &str) {
        let tokens = crate::utils::tokenize(text);
        let doc_len = tokens.len();

        for token in &tokens {
            self.inverted_index
                .entry(token.clone())
                .or_default()
                .push(doc_id.to_string());
        }

        self.documents.insert(doc_id.to_string(), tokens);
        self.doc_lengths.insert(doc_id.to_string(), doc_len);
        self.num_docs += 1;

        // Recalculate average document length
        let total_len: usize = self.doc_lengths.values().sum();
        self.avg_doc_len = total_len as f64 / self.num_docs as f64;
    }

    /// Search the index with a query string.
    pub fn search(&self, query: &str, k: usize) -> Vec<(String, f64)> {
        let query_tokens = crate::utils::tokenize(query);
        let mut scores: HashMap<String, f64> = HashMap::new();

        for token in &query_tokens {
            if let Some(doc_ids) = self.inverted_index.get(token) {
                let df = doc_ids.len() as f64;
                let idf = ((self.num_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

                // Count term frequency per document
                let mut tf_map: HashMap<&str, usize> = HashMap::new();
                for doc_id in doc_ids {
                    *tf_map.entry(doc_id).or_insert(0) += 1;
                }

                for (doc_id, tf) in tf_map {
                    let doc_len = *self.doc_lengths.get(doc_id).unwrap_or(&1) as f64;
                    let tf = tf as f64;
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator =
                        tf + self.k1 * (1.0 - self.b + self.b * doc_len / self.avg_doc_len);
                    let score = idf * numerator / denominator;
                    *scores.entry(doc_id.to_string()).or_insert(0.0) += score;
                }
            }
        }

        let mut results: Vec<(String, f64)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }

    /// Remove a document from the index.
    pub fn remove_document(&mut self, doc_id: &str) {
        if let Some(tokens) = self.documents.remove(doc_id) {
            // Remove from inverted index
            for token in &tokens {
                if let Some(doc_ids) = self.inverted_index.get_mut(token) {
                    doc_ids.retain(|id| id != doc_id);
                    if doc_ids.is_empty() {
                        self.inverted_index.remove(token);
                    }
                }
            }

            self.doc_lengths.remove(doc_id);
            self.num_docs = self.num_docs.saturating_sub(1);

            // Recalculate average document length
            if self.num_docs > 0 {
                let total_len: usize = self.doc_lengths.values().sum();
                self.avg_doc_len = total_len as f64 / self.num_docs as f64;
            } else {
                self.avg_doc_len = 0.0;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.num_docs
    }

    pub fn is_empty(&self) -> bool {
        self.num_docs == 0
    }
}

impl Default for BM25Index {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic() {
        let mut index = BM25Index::new();
        index.add_document("doc1", "the quick brown fox");
        index.add_document("doc2", "the lazy brown dog");
        index.add_document("doc3", "the quick red fox jumps over the lazy dog");

        let results = index.search("quick fox", 10);
        assert!(!results.is_empty());
        // doc1 and doc3 mention both "quick" and "fox"
        assert!(results.iter().any(|(id, _)| id == "doc1"));
        assert!(results.iter().any(|(id, _)| id == "doc3"));
    }

    #[test]
    fn test_bm25_empty() {
        let index = BM25Index::new();
        let results = index.search("test", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_ranking() {
        let mut index = BM25Index::new();
        index.add_document("high", "authentication login auth login auth");
        index.add_document("low", "database query connection pool");

        let results = index.search("authentication login", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "high");
    }

    #[test]
    fn test_bm25_remove_document() {
        let mut index = BM25Index::new();
        index.add_document("doc1", "the quick brown fox");
        index.add_document("doc2", "the lazy brown dog");
        assert_eq!(index.len(), 2);

        // Remove doc1
        index.remove_document("doc1");
        assert_eq!(index.len(), 1);

        // Searching for "fox" should no longer find doc1
        let results = index.search("fox", 10);
        assert!(results.iter().all(|(id, _)| id != "doc1"));

        // doc2 should still be searchable
        let results = index.search("dog", 10);
        assert!(results.iter().any(|(id, _)| id == "doc2"));

        // Removing non-existent doc is a no-op
        index.remove_document("nonexistent");
        assert_eq!(index.len(), 1);
    }
}
