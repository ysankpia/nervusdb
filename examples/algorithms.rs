// 样例 8：图算法
use nervusdb::{Direction, NervusDb};

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let db = NervusDb::open(dir.path().join("t.db"))?;
    db.run_cypher("CREATE (a:N)-[:ROAD {w: 1.0}]->(b:N)-[:ROAD {w: 2.0}]->(c:N)")?;

    if let Some((cost, path)) = db.dijkstra(1, 3, Some("ROAD")) {
        println!("cost={cost} path={path:?}");
    }
    let _ = db.bfs(1, 3, None);

    let has_cycle = db.try_has_cycle()?;
    let cycles = db.try_find_cycles()?;
    println!("{has_cycle} {cycles:?}");

    // PageRank 按分数降序返回；分数总和为 1.0
    let scores = db.pagerank();
    println!(
        "pagerank top: {:?}",
        scores.first().map(|s| (s.node_id, s.score))
    );
    let _ = db.pagerank_with(0.85, 100, 1e-6);

    // 弱连通分量：把边视为无向，按规模降序
    let components = db.weakly_connected_components();
    println!("components: {}", components.len());

    let sub = db.k_hop_subgraph(1, 2)?;
    println!("nodes={} edges={}", sub.nodes.len(), sub.edges.len());
    let _ = db.k_hop_subgraph_with(1, 2, Direction::Outgoing, None)?;
    Ok(())
}
