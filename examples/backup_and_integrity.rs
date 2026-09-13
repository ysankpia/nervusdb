// 样例 9：备份、空间与完整性
use nervusdb::NervusDb;

fn main() -> Result<(), nervusdb::GraphError> {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.db");
    let db = NervusDb::open(&p)?;
    db.run_cypher("CREATE (:P {v: 1})")?;

    let bytes = db.backup(dir.path().join("snap.db"))?;
    println!("copied {bytes} bytes");

    let report = db.vacuum()?;
    println!(
        "live={} reclaimable_prop_pages={}",
        report.nodes_live, report.free_property_pages
    );

    let rep = db.integrity_check()?;
    if !rep.is_ok() {
        for issue in &rep.issues {
            println!("{:?}: {}", issue.kind, issue.detail);
        }
    }
    Ok(())
}
