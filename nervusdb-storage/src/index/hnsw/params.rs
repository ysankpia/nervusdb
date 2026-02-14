#[derive(Clone, Debug)]
pub struct HnswParams {
    /// Max number of connections per element in all layers.
    pub m: usize,
    /// Size of the dynamic list for the set of candidates during construction.
    pub ef_construction: usize,
    /// Size of the dynamic list for the set of candidates during search.
    pub ef_search: usize,
}

impl Default for HnswParams {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            ef_search: 200,
        }
    }
}
