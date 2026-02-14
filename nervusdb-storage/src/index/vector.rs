use crate::Result;
use serde::{Deserialize, Serialize};

pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Interface for vector Similarity Search.
pub trait VectorIndex {
    /// Inserts a vector for the given internal node ID.
    fn insert(&mut self, id: u32, vector: Vec<f32>) -> Result<()>;

    /// Searches for the k-nearest neighbors of the given query vector.
    /// Returns a list of (node_id, distance).
    fn search(&mut self, query: &[f32], k: usize) -> Result<Vec<(u32, f32)>>;
}

/// A simple brute-force index for correctness verification.
#[derive(Default, Serialize, Deserialize)]
pub struct BruteForceIndex {
    vectors: Vec<(u32, Vec<f32>)>,
}

impl BruteForceIndex {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VectorIndex for BruteForceIndex {
    fn insert(&mut self, id: u32, vector: Vec<f32>) -> Result<()> {
        // Remove existing if present (naive scan)
        if let Some(pos) = self.vectors.iter().position(|(i, _)| *i == id) {
            self.vectors.remove(pos);
        }
        self.vectors.push((id, vector));
        Ok(())
    }

    fn search(&mut self, query: &[f32], k: usize) -> Result<Vec<(u32, f32)>> {
        let mut distances: Vec<(u32, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| {
                let dist = euclidean_distance(query, vec);
                (*id, dist)
            })
            .collect();

        // Sort by distance ascending (closest first)
        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        distances.truncate(k);

        Ok(distances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brute_force_index() {
        let mut index = BruteForceIndex::new();
        index.insert(1, vec![1.0, 0.0]).unwrap();
        index.insert(2, vec![0.0, 1.0]).unwrap();
        index.insert(3, vec![0.0, 0.0]).unwrap();

        let results = index.search(&[0.1, 0.1], 3).unwrap();
        assert_eq!(results[0].0, 3); // Origin is closest
        assert_eq!(results.len(), 3);
    }
}
