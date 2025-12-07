
fn delete_triple(txn: &mut redb::WriteTransaction, triple: &Triple) -> Result<bool> {
    let s = triple.subject_id;
    let p = triple.predicate_id;
    let o = triple.object_id;

    let mut spo = txn
        .open_table(TABLE_SPO)
        .map_err(|e| Error::Other(e.to_string()))?;

    if spo
        .get((s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?
        .is_none()
    {
        return Ok(false);
    }

    spo.remove((s, p, o))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut pos = txn
        .open_table(TABLE_POS)
        .map_err(|e| Error::Other(e.to_string()))?;
    pos.remove((p, o, s))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut osp = txn
        .open_table(TABLE_OSP)
        .map_err(|e| Error::Other(e.to_string()))?;
    osp.remove((o, s, p))
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(true)
}
