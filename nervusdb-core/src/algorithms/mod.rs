//! Graph algorithms for NervusDB
//!
//! This module provides graph algorithms that operate directly on the Hexastore.
//! All algorithms are implemented in Rust for maximum performance.
//!
//! # Available Algorithms
//!
//! ## Pathfinding
//! - [`bfs`] - Breadth-first search for shortest unweighted paths
//! - [`dijkstra`] - Dijkstra's algorithm for weighted shortest paths
//!
//! ## Centrality
//! - [`pagerank`] - PageRank algorithm for node importance

mod centrality;
mod pathfinding;

pub use centrality::{PageRankOptions, PageRankResult, pagerank};
pub use pathfinding::{PathResult, bfs_shortest_path, dijkstra_shortest_path};
