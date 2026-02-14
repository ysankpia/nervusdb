use crate::idmap::InternalNodeId;
use crate::snapshot::EdgeKey;
use std::collections::BTreeMap;

pub(crate) fn edges_for_src<'a>(
    edges_by_src: &'a BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    src: InternalNodeId,
) -> &'a [EdgeKey] {
    edges_by_src
        .get(&src)
        .map(|edges| edges.as_slice())
        .unwrap_or(&[])
}

pub(crate) fn edges_for_dst<'a>(
    edges_by_dst: &'a BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    dst: InternalNodeId,
) -> &'a [EdgeKey] {
    edges_by_dst
        .get(&dst)
        .map(|edges| edges.as_slice())
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::{edges_for_dst, edges_for_src};
    use crate::snapshot::EdgeKey;
    use std::collections::BTreeMap;

    #[test]
    fn edges_for_src_returns_existing_bucket_or_empty_slice() {
        let map = BTreeMap::from([(
            1,
            vec![
                EdgeKey {
                    src: 1,
                    rel: 2,
                    dst: 3,
                },
                EdgeKey {
                    src: 1,
                    rel: 4,
                    dst: 5,
                },
            ],
        )]);

        assert_eq!(edges_for_src(&map, 1).len(), 2);
        assert!(edges_for_src(&map, 9).is_empty());
    }

    #[test]
    fn edges_for_dst_returns_existing_bucket_or_empty_slice() {
        let map = BTreeMap::from([(
            3,
            vec![EdgeKey {
                src: 1,
                rel: 2,
                dst: 3,
            }],
        )]);

        assert_eq!(edges_for_dst(&map, 3).len(), 1);
        assert!(edges_for_dst(&map, 8).is_empty());
    }
}
