use crate::read_path_convert::internal_edge_to_api;
use crate::snapshot::EdgeKey;

pub struct ApiNeighborsIter<'a> {
    inner: Box<dyn Iterator<Item = EdgeKey> + 'a>,
}

impl<'a> ApiNeighborsIter<'a> {
    pub(crate) fn new(inner: Box<dyn Iterator<Item = EdgeKey> + 'a>) -> Self {
        Self { inner }
    }
}

impl<'a> Iterator for ApiNeighborsIter<'a> {
    type Item = nervusdb_api::EdgeKey;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(internal_edge_to_api)
    }
}

#[cfg(test)]
mod tests {
    use super::ApiNeighborsIter;
    use crate::snapshot::EdgeKey;

    #[test]
    fn api_neighbors_iter_converts_internal_edges() {
        let iter = vec![
            EdgeKey {
                src: 1,
                rel: 2,
                dst: 3,
            },
            EdgeKey {
                src: 4,
                rel: 5,
                dst: 6,
            },
        ]
        .into_iter();

        let api_edges: Vec<nervusdb_api::EdgeKey> = ApiNeighborsIter::new(Box::new(iter)).collect();
        assert_eq!(
            api_edges,
            vec![
                nervusdb_api::EdgeKey {
                    src: 1,
                    rel: 2,
                    dst: 3,
                },
                nervusdb_api::EdgeKey {
                    src: 4,
                    rel: 5,
                    dst: 6,
                },
            ]
        );
    }
}
