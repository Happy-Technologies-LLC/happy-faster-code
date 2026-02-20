use std::collections::HashMap;

/// Vector similarity search index.
/// Currently uses brute-force cosine similarity; can be swapped for HNSW later.
pub struct VectorIndex {
    /// Stored vectors (element_id -> vector)
    vectors: HashMap<String, Vec<f32>>,
    dimension: usize,
}

impl VectorIndex {
    pub fn new(dimension: usize) -> Self {
        Self {
            vectors: HashMap::new(),
            dimension,
        }
    }

    /// Add vectors with their element IDs.
    pub fn add(&mut self, ids: &[String], vectors: &[Vec<f32>]) {
        for (id, vec) in ids.iter().zip(vectors.iter()) {
            if vec.len() == self.dimension {
                self.vectors.insert(id.clone(), vec.clone());
            }
        }
    }

    /// Search for the k nearest neighbors of a query vector using cosine similarity.
    pub fn search(&self, query: &[f32], k: usize, min_score: f32) -> Vec<(String, f32)> {
        if query.len() != self.dimension || self.vectors.is_empty() {
            return Vec::new();
        }

        let query_norm = norm(query);
        if query_norm == 0.0 {
            return Vec::new();
        }

        let mut scores: Vec<(String, f32)> = self.vectors
            .iter()
            .filter_map(|(id, vec)| {
                let sim = cosine_similarity(query, vec, query_norm);
                if sim >= min_score {
                    Some((id.clone(), sim))
                } else {
                    None
                }
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine_similarity(a: &[f32], b: &[f32], a_norm: f32) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let b_norm = norm(b);
    if b_norm == 0.0 {
        return 0.0;
    }
    dot / (a_norm * b_norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_index_basic() {
        let mut index = VectorIndex::new(3);

        index.add(
            &["a".into(), "b".into(), "c".into()],
            &[
                vec![1.0, 0.0, 0.0],
                vec![0.0, 1.0, 0.0],
                vec![1.0, 0.1, 0.0],
            ],
        );

        assert_eq!(index.len(), 3);

        let results = index.search(&[1.0, 0.0, 0.0], 2, 0.0);
        assert!(!results.is_empty());
        // "a" should be the closest match (cosine sim = 1.0)
        assert_eq!(results[0].0, "a");
    }

    #[test]
    fn test_vector_index_empty() {
        let index = VectorIndex::new(3);
        let results = index.search(&[1.0, 0.0, 0.0], 5, 0.0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let a_norm = norm(&a);
        let sim = cosine_similarity(&a, &a, a_norm);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let a_norm = norm(&a);
        let sim = cosine_similarity(&a, &b, a_norm);
        assert!(sim.abs() < 1e-6);
    }
}
