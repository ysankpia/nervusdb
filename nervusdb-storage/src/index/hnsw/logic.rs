use super::params::HnswParams;
use super::storage::{GraphStorage, VectorStorage};
use crate::Result;
use crate::index::vector::euclidean_distance;
use ordered_float::OrderedFloat;
use rand::Rng;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

/// HNSW Index Implementation.
///
/// Generic over storage backends to support both in-memory and persistent modes.
#[derive(Debug)]
pub struct HnswIndex<V, G> {
    params: HnswParams,
    vector_store: V,
    graph_store: G,
    entry_point: Option<u32>, // InternalNodeId of entry point
    max_layer: u8,
}

impl<V, G> HnswIndex<V, G> {
    pub fn new(params: HnswParams, vector_store: V, graph_store: G) -> Self {
        Self {
            params,
            vector_store,
            graph_store,
            entry_point: None,
            max_layer: 0,
        }
    }

    pub fn load<Ctx>(
        params: HnswParams,
        vector_store: V,
        mut graph_store: G,
        ctx: &mut Ctx,
    ) -> Result<Self>
    where
        V: VectorStorage<Ctx>,
        G: GraphStorage<Ctx>,
    {
        let (entry_point, max_layer) = graph_store.get_meta(ctx)?;
        Ok(Self {
            params,
            vector_store,
            graph_store,
            entry_point,
            max_layer,
        })
    }

    fn random_level(&self) -> u8 {
        let mut rng = rand::thread_rng();
        let ml = 1.0 / (self.params.m as f64).ln();
        let r: f64 = rng.r#gen();
        ((-r.ln() * ml).floor() as u8).min(16) // Cap at 16 layers for safety
    }

    #[allow(clippy::type_complexity)]
    fn search_layer<Ctx>(
        &mut self,
        ctx: &mut Ctx,
        query: &[f32],
        entry_points: &[u32],
        ef: usize,
        layer: u8,
    ) -> Result<BinaryHeap<Reverse<(OrderedFloat<f32>, u32)>>>
    where
        V: VectorStorage<Ctx>,
        G: GraphStorage<Ctx>,
    {
        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut nearest_neighbors = BinaryHeap::new(); // Stores (Distance, ID) - MaxHeap

        for &ep in entry_points {
            if visited.insert(ep) {
                let vec = self.vector_store.get_vector(ctx, ep)?;
                let dist = euclidean_distance(query, &vec);
                let dist = OrderedFloat(dist);
                candidates.push(Reverse((dist, ep)));
                nearest_neighbors.push((dist, ep));
            }
        }

        while let Some(Reverse((d_c, c))) = candidates.pop() {
            let d_f = nearest_neighbors.peek().unwrap().0;
            if d_c > d_f && nearest_neighbors.len() >= ef {
                break;
            }

            let neighbors = self.graph_store.get_neighbors(ctx, layer, c)?;
            for &n in &neighbors {
                if visited.insert(n) {
                    let vec_n = self.vector_store.get_vector(ctx, n)?;
                    let dist_n = euclidean_distance(query, &vec_n);
                    let dist_n_ord = OrderedFloat(dist_n);

                    if nearest_neighbors.len() < ef
                        || dist_n_ord < nearest_neighbors.peek().unwrap().0
                    {
                        candidates.push(Reverse((dist_n_ord, n)));
                        nearest_neighbors.push((dist_n_ord, n));
                        if nearest_neighbors.len() > ef {
                            nearest_neighbors.pop();
                        }
                    }
                }
            }
        }

        // Convert MaxHeap (nearest_neighbors) to MinHeap (candidate style) for return
        let mut results = BinaryHeap::new();
        for (d, id) in nearest_neighbors {
            results.push(Reverse((d, id)));
        }
        Ok(results)
    }

    fn select_neighbors(
        &self,
        candidates: BinaryHeap<Reverse<(OrderedFloat<f32>, u32)>>,
        m: usize,
    ) -> Vec<u32> {
        // Simple heuristic: just take top M nearest
        let mut result = Vec::new();
        let mut heap = candidates;
        while let Some(Reverse((_, id))) = heap.pop() {
            result.push(id);
            if result.len() >= m {
                break;
            }
        }
        result
    }

    pub fn insert<Ctx>(&mut self, ctx: &mut Ctx, id: u32, vector: Vec<f32>) -> Result<()>
    where
        V: VectorStorage<Ctx>,
        G: GraphStorage<Ctx>,
    {
        // 1. Write vector
        self.vector_store.insert_vector(ctx, id, &vector)?;

        let level = self.random_level();
        let curr_obj = self.entry_point;

        // 2. Lock entry point logic
        if curr_obj.is_none() {
            self.entry_point = Some(id);
            self.max_layer = level;
            // Init empty layers
            for l in 0..=level {
                self.graph_store.set_neighbors(ctx, l, id, vec![])?;
            }

            // SAVE META
            self.graph_store
                .set_meta(ctx, self.entry_point, self.max_layer)?;

            return Ok(());
        }

        let curr_entry = curr_obj.unwrap();
        let curr_max_layer = self.max_layer;
        let mut curr_ep = curr_entry;

        // 3. Zoom down from max_layer to level+1
        let mut curr_dist = {
            let vec = self.vector_store.get_vector(ctx, curr_ep)?;
            OrderedFloat(euclidean_distance(&vector, &vec))
        };

        for l in (level + 1..=curr_max_layer).rev() {
            let mut changed = true;
            while changed {
                changed = false;
                let neighbors = self.graph_store.get_neighbors(ctx, l, curr_ep)?;
                for &n in &neighbors {
                    let vec_n = self.vector_store.get_vector(ctx, n)?;
                    let dist_n = OrderedFloat(euclidean_distance(&vector, &vec_n));
                    if dist_n < curr_dist {
                        curr_dist = dist_n;
                        curr_ep = n;
                        changed = true;
                    }
                }
            }
        }

        // 4. Insert from level down to 0
        let mut ep_candidates = vec![curr_ep];
        for l in (0..=level).rev() {
            // Search layer to get candidates
            let found =
                self.search_layer(ctx, &vector, &ep_candidates, self.params.ef_construction, l)?;

            // Select neighbors
            let neighbors = self.select_neighbors(found.clone(), self.params.m);

            // Store bidirectional connections
            self.graph_store
                .set_neighbors(ctx, l, id, neighbors.clone())?;

            // Add back-links
            for &n in &neighbors {
                let mut n_neighbors = self.graph_store.get_neighbors(ctx, l, n)?;
                if !n_neighbors.contains(&id) {
                    n_neighbors.push(id);
                    // Simplify: if too many, just keep top M
                    if n_neighbors.len() > self.params.m * 2 {
                        n_neighbors.truncate(self.params.m);
                    }
                    self.graph_store.set_neighbors(ctx, l, n, n_neighbors)?;
                }
            }

            // Update entry points for next layer (using search results)
            ep_candidates = found.into_iter().map(|Reverse((_, id))| id).collect();
        }

        // 5. Update global entry point if new node is at higher level
        if level > self.max_layer {
            self.max_layer = level;
            self.entry_point = Some(id);
        }

        // SAVE META
        self.graph_store
            .set_meta(ctx, self.entry_point, self.max_layer)?;

        Ok(())
    }

    pub fn search<Ctx>(&mut self, ctx: &mut Ctx, query: &[f32], k: usize) -> Result<Vec<(u32, f32)>>
    where
        V: VectorStorage<Ctx>,
        G: GraphStorage<Ctx>,
    {
        let ep_opt = self.entry_point;
        if ep_opt.is_none() {
            return Ok(Vec::new());
        }
        let mut curr_ep = ep_opt.unwrap();
        let max_layer = self.max_layer;

        // Zoom down
        let mut curr_dist = {
            let vec = self.vector_store.get_vector(ctx, curr_ep)?;
            OrderedFloat(euclidean_distance(query, &vec))
        };

        for l in (1..=max_layer).rev() {
            let mut changed = true;
            while changed {
                changed = false;
                let neighbors = self.graph_store.get_neighbors(ctx, l, curr_ep)?;
                for &n in &neighbors {
                    let vec_n = self.vector_store.get_vector(ctx, n)?;
                    let dist_n = OrderedFloat(euclidean_distance(query, &vec_n));
                    if dist_n < curr_dist {
                        curr_dist = dist_n;
                        curr_ep = n;
                        changed = true;
                    }
                }
            }
        }

        // Search base layer with ef_search
        let mut candidates = self.search_layer(ctx, query, &[curr_ep], self.params.ef_search, 0)?;

        let mut results = Vec::new();
        while let Some(Reverse((dist, id))) = candidates.pop() {
            results.push((id, dist.into_inner()));
            if results.len() >= k {
                break;
            }
        }
        Ok(results)
    }
}
