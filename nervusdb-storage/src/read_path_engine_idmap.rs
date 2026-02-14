use crate::idmap::{ExternalId, I2eRecord, IdMap, InternalNodeId, LabelId};
use std::sync::Mutex;

pub(crate) fn read_i2e_snapshot(idmap: &Mutex<IdMap>) -> Vec<I2eRecord> {
    idmap.lock().unwrap().get_i2e_snapshot()
}

pub(crate) fn lookup_internal_node_id(
    idmap: &Mutex<IdMap>,
    external_id: ExternalId,
) -> Option<InternalNodeId> {
    idmap.lock().unwrap().lookup(external_id)
}

pub(crate) fn read_i2l_snapshot(idmap: &Mutex<IdMap>) -> Vec<Vec<LabelId>> {
    idmap.lock().unwrap().get_i2l_snapshot()
}

#[cfg(test)]
mod tests {
    use super::{lookup_internal_node_id, read_i2e_snapshot, read_i2l_snapshot};
    use crate::idmap::IdMap;
    use crate::pager::Pager;
    use std::sync::Mutex;
    use tempfile::tempdir;

    #[test]
    fn read_i2e_snapshot_reads_records_from_idmap() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("idmap.ndb");
        let mut pager = Pager::open(&ndb).unwrap();

        let mut idmap = IdMap::load(&mut pager).unwrap();
        idmap.apply_create_node(&mut pager, 10, 1, 0).unwrap();
        idmap.apply_create_node(&mut pager, 20, 7, 1).unwrap();

        let idmap = Mutex::new(idmap);
        let snapshot = read_i2e_snapshot(&idmap);

        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].external_id, 10);
        assert_eq!(snapshot[0].label_id, 1);
        assert_eq!(snapshot[1].external_id, 20);
        assert_eq!(snapshot[1].label_id, 7);
    }

    #[test]
    fn read_i2e_snapshot_returns_owned_copy() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("idmap-copy.ndb");
        let mut pager = Pager::open(&ndb).unwrap();

        let mut idmap = IdMap::load(&mut pager).unwrap();
        idmap.apply_create_node(&mut pager, 100, 3, 0).unwrap();
        let idmap = Mutex::new(idmap);

        let mut first = read_i2e_snapshot(&idmap);
        first[0].flags = 42;

        let second = read_i2e_snapshot(&idmap);
        assert_eq!(second[0].flags, 0);
    }

    #[test]
    fn lookup_internal_node_id_reads_idmap_index() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("idmap-lookup.ndb");
        let mut pager = Pager::open(&ndb).unwrap();

        let mut idmap = IdMap::load(&mut pager).unwrap();
        idmap.apply_create_node(&mut pager, 10, 1, 0).unwrap();
        idmap.apply_create_node(&mut pager, 20, 2, 1).unwrap();
        let idmap = Mutex::new(idmap);

        assert_eq!(lookup_internal_node_id(&idmap, 10), Some(0));
        assert_eq!(lookup_internal_node_id(&idmap, 20), Some(1));
        assert_eq!(lookup_internal_node_id(&idmap, 999), None);
    }

    #[test]
    fn read_i2l_snapshot_returns_owned_copy() {
        let dir = tempdir().unwrap();
        let ndb = dir.path().join("idmap-labels.ndb");
        let mut pager = Pager::open(&ndb).unwrap();

        let mut idmap = IdMap::load(&mut pager).unwrap();
        idmap.apply_create_node(&mut pager, 10, 1, 0).unwrap();
        idmap.apply_add_label(&mut pager, 0, 5).unwrap();
        let idmap = Mutex::new(idmap);

        let mut first = read_i2l_snapshot(&idmap);
        first[0].push(99);

        let second = read_i2l_snapshot(&idmap);
        assert_eq!(second[0], vec![1, 5]);
    }
}
