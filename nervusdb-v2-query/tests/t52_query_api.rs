use nervusdb_v2_api::{EdgeKey, ExternalId, GraphSnapshot, InternalNodeId, RelTypeId};
use nervusdb_v2_query::{Params, Result, Value, prepare};
use std::collections::HashMap;

#[derive(Debug)]
struct FakeSnapshot {
    nodes: Vec<InternalNodeId>,
    edges_by_src: HashMap<InternalNodeId, Vec<EdgeKey>>,
    external: HashMap<InternalNodeId, ExternalId>,
}

#[derive(Debug)]
struct FakeNeighbors<'a> {
    edges: &'a [EdgeKey],
    rel: Option<RelTypeId>,
    idx: usize,
}

impl<'a> Iterator for FakeNeighbors<'a> {
    type Item = EdgeKey;

    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.edges.len() {
            let e = self.edges[self.idx];
            self.idx += 1;
            if let Some(rel) = self.rel
                && e.rel != rel
            {
                continue;
            }
            return Some(e);
        }
        None
    }
}

impl GraphSnapshot for FakeSnapshot {
    type Neighbors<'a>
        = FakeNeighbors<'a>
    where
        Self: 'a;

    fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> Self::Neighbors<'_> {
        let edges = self
            .edges_by_src
            .get(&src)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        FakeNeighbors { edges, rel, idx: 0 }
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = InternalNodeId> + '_> {
        Box::new(self.nodes.iter().copied())
    }

    fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId> {
        self.external.get(&iid).copied()
    }
}

#[test]
fn t52_prepare_and_execute_return_one() {
    let q = prepare("RETURN 1").unwrap();
    let snap = FakeSnapshot {
        nodes: vec![],
        edges_by_src: HashMap::new(),
        external: HashMap::new(),
    };
    let rows: Vec<_> = q
        .execute_streaming(&snap, &Params::new())
        .collect::<Result<_>>()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].columns()[0].1, Value::Int(1));
}

#[test]
fn t52_prepare_and_execute_match_out() {
    let q = prepare("MATCH (n)-[:7]->(m) RETURN n, m LIMIT 10").unwrap();

    let mut edges_by_src = HashMap::new();
    edges_by_src.insert(
        0,
        vec![EdgeKey {
            src: 0,
            rel: 7,
            dst: 1,
        }],
    );

    let snap = FakeSnapshot {
        nodes: vec![0, 1],
        edges_by_src,
        external: HashMap::new(),
    };

    let rows: Vec<_> = q
        .execute_streaming(&snap, &Params::new())
        .collect::<Result<_>>()
        .unwrap();
    assert_eq!(rows.len(), 1);

    let cols = rows[0].columns();
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0].0, "n");
    assert_eq!(cols[1].0, "m");
}
