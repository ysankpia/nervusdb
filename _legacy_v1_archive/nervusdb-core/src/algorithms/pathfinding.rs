//! Pathfinding algorithms for graph traversal
//!
//! Provides BFS and Dijkstra algorithms operating directly on the Hexastore.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use crate::Triple;
use crate::storage::Hexastore;

/// Result of a pathfinding operation
#[derive(Debug, Clone)]
pub struct PathResult {
    /// The path as a sequence of node IDs
    pub path: Vec<u64>,
    /// Total cost/distance of the path
    pub cost: f64,
    /// Number of hops (edges) in the path
    pub hops: usize,
}

/// Entry for Dijkstra's priority queue
#[derive(Debug, Clone)]
struct DijkstraEntry {
    node_id: u64,
    cost: f64,
}

impl PartialEq for DijkstraEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}

impl Eq for DijkstraEntry {}

impl PartialOrd for DijkstraEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DijkstraEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

/// Breadth-First Search for shortest unweighted path
///
/// # Arguments
/// * `store` - The Hexastore to search
/// * `start_id` - Starting node ID
/// * `end_id` - Target node ID  
/// * `predicate_id` - Optional predicate ID to filter edges
/// * `max_hops` - Maximum path length
/// * `bidirectional` - Whether to traverse edges in both directions
///
/// # Returns
/// `Some(PathResult)` if a path exists, `None` otherwise
pub fn bfs_shortest_path(
    store: &dyn Hexastore,
    start_id: u64,
    end_id: u64,
    predicate_id: Option<u64>,
    max_hops: usize,
    bidirectional: bool,
) -> Option<PathResult> {
    if start_id == end_id {
        return Some(PathResult {
            path: vec![start_id],
            cost: 0.0,
            hops: 0,
        });
    }

    let mut visited: HashSet<u64> = HashSet::new();
    let mut queue: VecDeque<(u64, Vec<u64>)> = VecDeque::new();

    visited.insert(start_id);
    queue.push_back((start_id, vec![start_id]));

    while let Some((current, path)) = queue.pop_front() {
        if path.len() > max_hops {
            continue;
        }

        // Get outgoing edges
        let neighbors = get_neighbors(store, current, predicate_id, bidirectional);

        for neighbor_id in neighbors {
            if neighbor_id == end_id {
                let mut final_path = path.clone();
                final_path.push(neighbor_id);
                return Some(PathResult {
                    hops: final_path.len() - 1,
                    cost: (final_path.len() - 1) as f64,
                    path: final_path,
                });
            }

            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id);
                let mut new_path = path.clone();
                new_path.push(neighbor_id);
                queue.push_back((neighbor_id, new_path));
            }
        }
    }

    None
}

/// Dijkstra's algorithm for weighted shortest path
///
/// # Arguments
/// * `store` - The Hexastore to search
/// * `start_id` - Starting node ID
/// * `end_id` - Target node ID
/// * `predicate_id` - Optional predicate ID to filter edges
/// * `weight_fn` - Function to get edge weight from (subject, predicate, object)
/// * `max_hops` - Maximum path length
///
/// # Returns
/// `Some(PathResult)` if a path exists, `None` otherwise
pub fn dijkstra_shortest_path<F>(
    store: &dyn Hexastore,
    start_id: u64,
    end_id: u64,
    predicate_id: Option<u64>,
    weight_fn: F,
    max_hops: usize,
) -> Option<PathResult>
where
    F: Fn(u64, u64, u64) -> f64,
{
    if start_id == end_id {
        return Some(PathResult {
            path: vec![start_id],
            cost: 0.0,
            hops: 0,
        });
    }

    let mut distances: HashMap<u64, f64> = HashMap::new();
    let mut predecessors: HashMap<u64, u64> = HashMap::new();
    let mut hops_count: HashMap<u64, usize> = HashMap::new();
    let mut heap: BinaryHeap<DijkstraEntry> = BinaryHeap::new();

    distances.insert(start_id, 0.0);
    hops_count.insert(start_id, 0);
    heap.push(DijkstraEntry {
        node_id: start_id,
        cost: 0.0,
    });

    while let Some(DijkstraEntry { node_id, cost }) = heap.pop() {
        // Skip if we've found a better path
        if let Some(&best) = distances.get(&node_id)
            && cost > best
        {
            continue;
        }

        // Check hop limit
        let current_hops = *hops_count.get(&node_id).unwrap_or(&0);
        if current_hops >= max_hops {
            continue;
        }

        // Found the target
        if node_id == end_id {
            return Some(reconstruct_path(&predecessors, start_id, end_id, cost));
        }

        // Explore neighbors
        let triples = get_outgoing_triples(store, node_id, predicate_id);

        for triple in triples {
            let neighbor_id = triple.object_id;
            let edge_weight = weight_fn(triple.subject_id, triple.predicate_id, triple.object_id);
            let new_cost = cost + edge_weight;

            let is_better = distances
                .get(&neighbor_id)
                .is_none_or(|&current| new_cost < current);

            if is_better {
                distances.insert(neighbor_id, new_cost);
                predecessors.insert(neighbor_id, node_id);
                hops_count.insert(neighbor_id, current_hops + 1);
                heap.push(DijkstraEntry {
                    node_id: neighbor_id,
                    cost: new_cost,
                });
            }
        }
    }

    // Check if we reached the target through any path
    if let Some(&cost) = distances.get(&end_id) {
        return Some(reconstruct_path(&predecessors, start_id, end_id, cost));
    }

    None
}

/// Get all neighbors of a node
fn get_neighbors(
    store: &dyn Hexastore,
    node_id: u64,
    predicate_id: Option<u64>,
    bidirectional: bool,
) -> Vec<u64> {
    let mut neighbors = Vec::new();

    // Outgoing edges: node -> neighbor
    let outgoing = store.query(Some(node_id), predicate_id, None);
    for triple in outgoing {
        neighbors.push(triple.object_id);
    }

    // Incoming edges: neighbor -> node (if bidirectional)
    if bidirectional {
        let incoming = store.query(None, predicate_id, Some(node_id));
        for triple in incoming {
            neighbors.push(triple.subject_id);
        }
    }

    neighbors
}

/// Get outgoing triples from a node
fn get_outgoing_triples(
    store: &dyn Hexastore,
    node_id: u64,
    predicate_id: Option<u64>,
) -> Vec<Triple> {
    store.query(Some(node_id), predicate_id, None).collect()
}

/// Reconstruct path from predecessors map
fn reconstruct_path(
    predecessors: &HashMap<u64, u64>,
    start_id: u64,
    end_id: u64,
    cost: f64,
) -> PathResult {
    let mut path = vec![end_id];
    let mut current = end_id;

    while current != start_id {
        if let Some(&prev) = predecessors.get(&current) {
            path.push(prev);
            current = prev;
        } else {
            break;
        }
    }

    path.reverse();

    PathResult {
        hops: path.len() - 1,
        cost,
        path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::MemoryHexastore;

    fn create_test_graph() -> MemoryHexastore {
        let mut store = MemoryHexastore::new();

        // Create a simple graph:
        // A(1) -> B(2) -> C(3)
        //   \-> D(4) -> C(3)
        // With predicates: knows(10), follows(11)

        store.insert(&Triple::new(1, 10, 2)).unwrap(); // A knows B
        store.insert(&Triple::new(2, 10, 3)).unwrap(); // B knows C
        store.insert(&Triple::new(1, 11, 4)).unwrap(); // A follows D
        store.insert(&Triple::new(4, 10, 3)).unwrap(); // D knows C

        store
    }

    #[test]
    fn test_bfs_direct_path() {
        let store = create_test_graph();

        let result = bfs_shortest_path(&store, 1, 2, None, 10, false);
        assert!(result.is_some());

        let path = result.unwrap();
        assert_eq!(path.path, vec![1, 2]);
        assert_eq!(path.hops, 1);
    }

    #[test]
    fn test_bfs_two_hop_path() {
        let store = create_test_graph();

        let result = bfs_shortest_path(&store, 1, 3, None, 10, false);
        assert!(result.is_some());

        let path = result.unwrap();
        assert_eq!(path.hops, 2);
        // Should find A -> B -> C or A -> D -> C (both are length 2)
        assert!(path.path.len() == 3);
        assert_eq!(path.path[0], 1);
        assert_eq!(path.path[2], 3);
    }

    #[test]
    fn test_bfs_no_path() {
        let store = create_test_graph();

        // No path from C to A (no reverse edges without bidirectional)
        let result = bfs_shortest_path(&store, 3, 1, None, 10, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_bfs_max_hops_limit() {
        let store = create_test_graph();

        // A -> C requires 2 hops, but we limit to 1
        let result = bfs_shortest_path(&store, 1, 3, None, 1, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_bfs_same_node() {
        let store = create_test_graph();

        let result = bfs_shortest_path(&store, 1, 1, None, 10, false);
        assert!(result.is_some());

        let path = result.unwrap();
        assert_eq!(path.path, vec![1]);
        assert_eq!(path.hops, 0);
    }

    #[test]
    fn test_dijkstra_uniform_weights() {
        let store = create_test_graph();

        let result = dijkstra_shortest_path(
            &store,
            1,
            3,
            None,
            |_, _, _| 1.0, // uniform weights
            10,
        );

        assert!(result.is_some());
        let path = result.unwrap();
        assert_eq!(path.cost, 2.0);
        assert_eq!(path.hops, 2);
    }
}
