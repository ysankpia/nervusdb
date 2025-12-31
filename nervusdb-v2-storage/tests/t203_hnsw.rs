use nervusdb_v2_storage::index::btree::BTree;
use nervusdb_v2_storage::index::hnsw::storage::{PersistentGraphStorage, PersistentVectorStorage};
use nervusdb_v2_storage::index::hnsw::{HnswIndex, HnswParams};
use nervusdb_v2_storage::index::vector::VectorIndex;
use nervusdb_v2_storage::pager::Pager;
use tempfile::tempdir;

#[test]
fn test_hnsw_persistence() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_hnsw.ndb");
    let mut btree_root_opt: Option<u64> = None;

    // 1. Create and Insert
    {
        println!("Initializing DB...");
        let mut pager = Pager::open(&db_path).unwrap();
        // Since we need to use Pager for BTree creation AND then pass it to HnswIndex,
        // we execute BTree creation first.
        let btree = BTree::create(&mut pager).unwrap();
        btree_root_opt = Some(btree.root().as_u64());

        let v_store = PersistentVectorStorage::new(BTree::load(btree.root()));
        let g_store = PersistentGraphStorage::new(BTree::load(btree.root()));

        let params = HnswParams {
            m: 16,
            ef_construction: 200,
            ef_search: 200,
        };

        // HnswIndex no longer takes ownership of Pager
        let mut index = HnswIndex::load(params, v_store, g_store, &mut pager).unwrap();

        println!("Inserting vectors...");
        // Insert: (1, [1.0, 0.0]), (2, [0.0, 1.0]), (3, [0.0, 0.0])
        index.insert(&mut pager, 1, vec![1.0, 0.0]).unwrap();
        index.insert(&mut pager, 2, vec![0.0, 1.0]).unwrap();
        index.insert(&mut pager, 3, vec![0.0, 0.0]).unwrap(); // Origin
        index.insert(&mut pager, 4, vec![0.9, 0.1]).unwrap(); // Close to 1

        // Verify In-Memory Search (before drop)
        let res = index.search(&mut pager, &[0.1, 0.1], 3).unwrap();
        println!("Search results (warm): {:?}", res);
        assert_eq!(res[0].0, 3); // 3 is [0,0], closest to [0.1, 0.1]
    }

    println!("Reopening DB...");

    // 2. Reopen and Search (verify persistence)
    {
        let mut pager = Pager::open(&db_path).unwrap();
        let root = nervusdb_v2_storage::pager::PageId::new(btree_root_opt.unwrap());

        let v_store = PersistentVectorStorage::new(BTree::load(root));
        let g_store = PersistentGraphStorage::new(BTree::load(root));

        let params = HnswParams {
            m: 16,
            ef_construction: 200,
            ef_search: 200,
        };

        let mut index = HnswIndex::load(params, v_store, g_store, &mut pager).unwrap();

        // Search again
        // Query near 1: [0.95, 0.05]
        let res = index.search(&mut pager, &[0.95, 0.05], 2).unwrap();
        println!("Search results (cold): {:?}", res);

        // Should find 1 and 4
        let ids: Vec<u32> = res.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&4));

        // Query near origin
        let res2 = index.search(&mut pager, &[0.0, 0.0], 1).unwrap();
        assert_eq!(res2[0].0, 3);
    }
}
