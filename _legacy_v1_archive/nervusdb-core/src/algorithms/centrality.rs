//! Centrality algorithms for measuring node importance
//!
//! Provides PageRank and other centrality measures.

use std::collections::HashMap;

use crate::storage::Hexastore;

/// Options for PageRank computation
#[derive(Debug, Clone)]
pub struct PageRankOptions {
    /// Damping factor (probability of following a link vs random jump)
    /// Default: 0.85
    pub damping: f64,
    /// Maximum number of iterations
    /// Default: 100
    pub max_iterations: usize,
    /// Convergence tolerance (stop when max change < tolerance)
    /// Default: 1e-6
    pub tolerance: f64,
}

impl Default for PageRankOptions {
    fn default() -> Self {
        Self {
            damping: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
        }
    }
}

/// Result of PageRank computation
#[derive(Debug, Clone)]
pub struct PageRankResult {
    /// PageRank scores for each node
    pub scores: HashMap<u64, f64>,
    /// Number of iterations performed
    pub iterations: usize,
    /// Whether the algorithm converged
    pub converged: bool,
    /// Maximum change in the final iteration
    pub max_change: f64,
}

/// Compute PageRank for all nodes in the graph
///
/// # Arguments
/// * `store` - The Hexastore containing the graph
/// * `predicate_id` - Optional predicate ID to filter edges
/// * `options` - PageRank configuration options
///
/// # Returns
/// `PageRankResult` containing scores for all nodes
pub fn pagerank(
    store: &dyn Hexastore,
    predicate_id: Option<u64>,
    options: PageRankOptions,
) -> PageRankResult {
    // Collect all nodes and build adjacency information
    let (nodes, outgoing, incoming) = build_graph_structure(store, predicate_id);

    let n = nodes.len();
    if n == 0 {
        return PageRankResult {
            scores: HashMap::new(),
            iterations: 0,
            converged: true,
            max_change: 0.0,
        };
    }

    let initial_score = 1.0 / n as f64;
    let mut scores: HashMap<u64, f64> = nodes.iter().map(|&id| (id, initial_score)).collect();
    let mut new_scores: HashMap<u64, f64> = HashMap::with_capacity(n);

    let damping = options.damping;
    let random_jump = (1.0 - damping) / n as f64;

    let mut iterations = 0;
    let mut converged = false;
    let mut max_change = f64::MAX;

    for _ in 0..options.max_iterations {
        iterations += 1;
        max_change = 0.0;

        // Calculate dangling node contribution (nodes with no outgoing edges)
        let dangling_sum: f64 = nodes
            .iter()
            .filter(|&&id| outgoing.get(&id).is_none_or(|v| v.is_empty()))
            .map(|&id| scores.get(&id).unwrap_or(&0.0))
            .sum();
        let dangling_contribution = damping * dangling_sum / n as f64;

        // Update scores for each node
        for &node_id in &nodes {
            let mut link_score = 0.0;

            // Sum contributions from incoming links
            if let Some(in_neighbors) = incoming.get(&node_id) {
                for &neighbor_id in in_neighbors {
                    let neighbor_score = *scores.get(&neighbor_id).unwrap_or(&0.0);
                    let neighbor_out_degree =
                        outgoing.get(&neighbor_id).map_or(1, |v| v.len().max(1));
                    link_score += neighbor_score / neighbor_out_degree as f64;
                }
            }

            let new_score = random_jump + dangling_contribution + damping * link_score;
            let old_score = *scores.get(&node_id).unwrap_or(&0.0);
            let change = (new_score - old_score).abs();
            max_change = max_change.max(change);

            new_scores.insert(node_id, new_score);
        }

        // Swap scores
        std::mem::swap(&mut scores, &mut new_scores);
        new_scores.clear();

        // Check convergence
        if max_change < options.tolerance {
            converged = true;
            break;
        }
    }

    // Normalize scores to sum to 1
    let total: f64 = scores.values().sum();
    if total > 0.0 {
        for score in scores.values_mut() {
            *score /= total;
        }
    }

    PageRankResult {
        scores,
        iterations,
        converged,
        max_change,
    }
}

/// Graph adjacency structure
type GraphStructure = (Vec<u64>, HashMap<u64, Vec<u64>>, HashMap<u64, Vec<u64>>);

/// Build graph structure from Hexastore
fn build_graph_structure(store: &dyn Hexastore, predicate_id: Option<u64>) -> GraphStructure {
    let mut nodes_set = std::collections::HashSet::new();
    let mut outgoing: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut incoming: HashMap<u64, Vec<u64>> = HashMap::new();

    // Iterate over all triples
    let iter = store.query(None, predicate_id, None);

    for triple in iter {
        let subject = triple.subject_id;
        let object = triple.object_id;

        nodes_set.insert(subject);
        nodes_set.insert(object);

        outgoing.entry(subject).or_default().push(object);
        incoming.entry(object).or_default().push(subject);
    }

    let nodes: Vec<u64> = nodes_set.into_iter().collect();
    (nodes, outgoing, incoming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Triple;
    use crate::storage::memory::MemoryHexastore;

    fn create_simple_graph() -> MemoryHexastore {
        let mut store = MemoryHexastore::new();

        // Simple graph: A -> B -> C, B -> D
        // A(1), B(2), C(3), D(4), predicate(10)
        store.insert(&Triple::new(1, 10, 2)).unwrap(); // A -> B
        store.insert(&Triple::new(2, 10, 3)).unwrap(); // B -> C
        store.insert(&Triple::new(2, 10, 4)).unwrap(); // B -> D

        store
    }

    fn create_cyclic_graph() -> MemoryHexastore {
        let mut store = MemoryHexastore::new();

        // Cyclic graph: A -> B -> C -> A
        store.insert(&Triple::new(1, 10, 2)).unwrap();
        store.insert(&Triple::new(2, 10, 3)).unwrap();
        store.insert(&Triple::new(3, 10, 1)).unwrap();

        store
    }

    #[test]
    fn test_pagerank_simple() {
        let store = create_simple_graph();

        let result = pagerank(&store, None, PageRankOptions::default());

        assert!(result.converged);
        assert!(result.iterations > 0);

        // All scores should be positive
        for &score in result.scores.values() {
            assert!(score > 0.0);
        }

        // Scores should sum to approximately 1
        let total: f64 = result.scores.values().sum();
        assert!((total - 1.0).abs() < 0.001);

        // C and D should have higher scores (they receive links)
        let score_a = *result.scores.get(&1).unwrap_or(&0.0);
        let score_b = *result.scores.get(&2).unwrap_or(&0.0);
        let score_c = *result.scores.get(&3).unwrap_or(&0.0);

        // B should have higher score than A (B receives a link from A)
        assert!(score_b > score_a);
        // C should have some score (receives link from B)
        assert!(score_c > 0.0);
    }

    #[test]
    fn test_pagerank_cyclic() {
        let store = create_cyclic_graph();

        let result = pagerank(&store, None, PageRankOptions::default());

        assert!(result.converged);

        // In a symmetric cycle, all nodes should have equal PageRank
        let scores: Vec<f64> = result.scores.values().cloned().collect();
        let first = scores[0];
        for score in &scores {
            assert!((score - first).abs() < 0.01);
        }
    }

    #[test]
    fn test_pagerank_empty_graph() {
        let store = MemoryHexastore::new();

        let result = pagerank(&store, None, PageRankOptions::default());

        assert!(result.converged);
        assert_eq!(result.iterations, 0);
        assert!(result.scores.is_empty());
    }

    #[test]
    fn test_pagerank_custom_options() {
        let store = create_simple_graph();

        let options = PageRankOptions {
            damping: 0.5,
            max_iterations: 10,
            tolerance: 1e-3,
        };

        let result = pagerank(&store, None, options);

        assert!(result.iterations <= 10);

        let total: f64 = result.scores.values().sum();
        assert!((total - 1.0).abs() < 0.001);
    }
}
