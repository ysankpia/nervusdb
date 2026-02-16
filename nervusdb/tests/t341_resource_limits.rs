use nervusdb::Db;
use nervusdb::query::{ExecuteOptions, Params, Value, prepare};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_execute_options_default_balanced_profile() {
    let params = Params::new();
    let opts = params.execute_options();
    assert_eq!(opts.max_intermediate_rows, 500_000);
    assert_eq!(opts.max_collection_items, 200_000);
    assert_eq!(opts.soft_timeout_ms, 5_000);
    assert_eq!(opts.max_apply_rows_per_outer, 200_000);
}

#[test]
fn test_range_exceed_collection_limit_raises_resource_limit() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t341_range_limit.ndb"))?;
    let snapshot = db.snapshot();

    let mut params = Params::new();
    params.set_execute_options(ExecuteOptions {
        max_collection_items: 10,
        ..ExecuteOptions::default()
    });

    let prepared = prepare("RETURN range(0, 100) AS xs")?;
    let err = prepared
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .expect_err("range list should trip collection limit")
        .to_string();

    assert!(err.contains("ResourceLimitExceeded"), "err={err}");
    assert!(err.contains("CollectionItems"), "err={err}");
    Ok(())
}

#[test]
fn test_default_limits_keep_tck_sum_range_case_working() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t341_range_tck_compat.ndb"))?;
    let snapshot = db.snapshot();

    let params = Params::new();
    let prepared = prepare("UNWIND range(0, 1000000) AS i RETURN sum(i) AS s")?;
    let rows = prepared
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("s"), Some(&Value::Int(500000500000)));
    Ok(())
}

#[test]
fn test_intermediate_rows_limit_raises_resource_limit() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t341_rows_limit.ndb"))?;
    let snapshot = db.snapshot();

    let mut params = Params::new();
    params.set_execute_options(ExecuteOptions {
        max_intermediate_rows: 5,
        ..ExecuteOptions::default()
    });

    let prepared = prepare("UNWIND range(1, 50) AS x RETURN x")?;
    let err = prepared
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .expect_err("intermediate rows should trip row budget")
        .to_string();

    assert!(err.contains("ResourceLimitExceeded"), "err={err}");
    assert!(err.contains("IntermediateRows"), "err={err}");
    Ok(())
}

#[test]
fn test_soft_timeout_raises_resource_limit() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t341_timeout_limit.ndb"))?;
    let snapshot = db.snapshot();

    let mut params = Params::new();
    params.set_execute_options(ExecuteOptions {
        soft_timeout_ms: 1,
        ..ExecuteOptions::default()
    });

    let prepared = prepare("RETURN 1 AS n")?;
    let iter = prepared.execute_streaming(&snapshot, &params);
    thread::sleep(Duration::from_millis(10));
    let err = iter
        .collect::<Result<Vec<_>, _>>()
        .expect_err("query should time out before first pull")
        .to_string();

    assert!(err.contains("ResourceLimitExceeded"), "err={err}");
    assert!(err.contains("Timeout"), "err={err}");
    Ok(())
}

#[test]
fn test_apply_rows_per_outer_limit_raises_resource_limit() -> nervusdb::Result<()> {
    let dir = tempdir()?;
    let db = Db::open(dir.path().join("t341_apply_limit.ndb"))?;
    let snapshot = db.snapshot();

    let mut params = Params::new();
    params.set_execute_options(ExecuteOptions {
        max_apply_rows_per_outer: 3,
        ..ExecuteOptions::default()
    });

    let query = "UNWIND [1, 2] AS n \
                 CALL { WITH n UNWIND range(1, 10) AS x RETURN x } \
                 RETURN n, x";
    let prepared = prepare(query)?;
    let err = prepared
        .execute_streaming(&snapshot, &params)
        .collect::<Result<Vec<_>, _>>()
        .expect_err("apply subquery should trip per-outer-row cap")
        .to_string();

    assert!(err.contains("ResourceLimitExceeded"), "err={err}");
    assert!(err.contains("ApplyRowsPerOuter"), "err={err}");
    Ok(())
}
