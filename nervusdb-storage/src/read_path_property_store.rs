use crate::idmap::InternalNodeId;
use crate::index::btree::BTree;
use crate::pager::{PageId, Pager};
use crate::property::PropertyValue;
use std::collections::BTreeMap;

pub(crate) fn read_node_property_from_store(
    pager: &Pager,
    properties_root: u64,
    node: InternalNodeId,
    key: &str,
) -> Option<PropertyValue> {
    if properties_root == 0 {
        return None;
    }

    let tree = BTree::load(PageId::new(properties_root));

    let mut btree_key = Vec::with_capacity(1 + 4 + 4 + key.len());
    btree_key.push(0u8); // Tag 0: Node Property
    btree_key.extend_from_slice(&node.to_be_bytes());
    btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
    btree_key.extend_from_slice(key.as_bytes());

    let blob_id = {
        let mut cursor = tree.cursor_lower_bound(pager, &btree_key).ok()?;
        if cursor.is_valid().ok()? {
            let got_key = cursor.key().ok()?;
            if got_key == btree_key {
                cursor.payload().ok()
            } else {
                None
            }
        } else {
            None
        }
    };

    blob_id.and_then(|blob_id| decode_property_blob(pager, blob_id))
}

pub(crate) fn read_edge_property_from_store(
    pager: &Pager,
    properties_root: u64,
    edge: nervusdb_api::EdgeKey,
    key: &str,
) -> Option<PropertyValue> {
    if properties_root == 0 {
        return None;
    }

    let tree = BTree::load(PageId::new(properties_root));

    let mut btree_key = Vec::with_capacity(1 + 4 + 4 + 4 + 4 + key.len());
    btree_key.push(1u8); // Tag 1: Edge Property
    btree_key.extend_from_slice(&edge.src.to_be_bytes());
    btree_key.extend_from_slice(&edge.rel.to_be_bytes());
    btree_key.extend_from_slice(&edge.dst.to_be_bytes());
    btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
    btree_key.extend_from_slice(key.as_bytes());

    let blob_id = {
        let mut cursor = tree.cursor_lower_bound(pager, &btree_key).ok()?;
        if cursor.is_valid().ok()? {
            let got_key = cursor.key().ok()?;
            if got_key == btree_key {
                cursor.payload().ok()
            } else {
                None
            }
        } else {
            None
        }
    };

    blob_id.and_then(|blob_id| decode_property_blob(pager, blob_id))
}

pub(crate) fn extend_node_properties_from_store(
    pager: &Pager,
    properties_root: u64,
    node: InternalNodeId,
    props: &mut BTreeMap<String, PropertyValue>,
) -> Option<()> {
    if properties_root == 0 {
        return Some(());
    }

    let tree = BTree::load(PageId::new(properties_root));

    // Prefix search for [tag=0: 1B][node: u32 BE]
    let mut prefix = Vec::with_capacity(5);
    prefix.push(0u8);
    prefix.extend_from_slice(&node.to_be_bytes());

    let mut to_fetch = Vec::new();
    {
        let mut cursor = tree.cursor_lower_bound(pager, &prefix).ok()?;
        while cursor.is_valid().ok()? {
            let key = cursor.key().ok()?;
            if !key.starts_with(&prefix) {
                break;
            }

            // Key format: [tag: 1B][node: 4B][key_len: 4B][key_bytes]
            if key.len() < 9 {
                break;
            }
            let key_len = u32::from_be_bytes(key[5..9].try_into().unwrap()) as usize;
            let key_name = String::from_utf8(key[9..9 + key_len].to_vec()).ok()?;

            if !props.contains_key(&key_name) {
                to_fetch.push((key_name, cursor.payload().ok()?));
            }

            if !cursor.advance().ok()? {
                break;
            }
        }
    }

    for (key_name, blob_id) in to_fetch {
        let storage_val = decode_property_blob(pager, blob_id)?;
        props.insert(key_name, storage_val);
    }

    Some(())
}

pub(crate) fn extend_edge_properties_from_store(
    pager: &Pager,
    properties_root: u64,
    edge: nervusdb_api::EdgeKey,
    props: &mut BTreeMap<String, PropertyValue>,
) -> Option<()> {
    if properties_root == 0 {
        return Some(());
    }

    let tree = BTree::load(PageId::new(properties_root));

    // Prefix search for [tag=1: 1B][src: 4B][rel: 4B][dst: 4B]
    let mut prefix = Vec::with_capacity(13);
    prefix.push(1u8);
    prefix.extend_from_slice(&edge.src.to_be_bytes());
    prefix.extend_from_slice(&edge.rel.to_be_bytes());
    prefix.extend_from_slice(&edge.dst.to_be_bytes());

    let mut to_fetch = Vec::new();
    {
        let mut cursor = tree.cursor_lower_bound(pager, &prefix).ok()?;
        while cursor.is_valid().ok()? {
            let key = cursor.key().ok()?;
            if !key.starts_with(&prefix) {
                break;
            }

            // Key format: [tag: 1B][src: 4B][rel: 4B][dst: 4B][key_len: 4B][key_bytes]
            if key.len() < 17 {
                break;
            }
            let key_len = u32::from_be_bytes(key[13..17].try_into().unwrap()) as usize;
            let key_name = String::from_utf8(key[17..17 + key_len].to_vec()).ok()?;

            if !props.contains_key(&key_name) {
                to_fetch.push((key_name, cursor.payload().ok()?));
            }

            if !cursor.advance().ok()? {
                break;
            }
        }
    }

    for (key_name, blob_id) in to_fetch {
        let storage_val = decode_property_blob(pager, blob_id)?;
        props.insert(key_name, storage_val);
    }

    Some(())
}

fn decode_property_blob(pager: &Pager, blob_id: u64) -> Option<PropertyValue> {
    let bytes = crate::blob_store::BlobStore::read(pager, blob_id).ok()?;
    crate::property::PropertyValue::decode(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        extend_edge_properties_from_store, extend_node_properties_from_store,
        read_edge_property_from_store, read_node_property_from_store,
    };
    use crate::index::btree::BTree;
    use crate::pager::Pager;
    use crate::property::PropertyValue;
    use std::collections::BTreeMap;

    fn insert_node_prop(
        pager: &mut Pager,
        tree: &mut BTree,
        node: u32,
        key: &str,
        value: PropertyValue,
    ) {
        let mut btree_key = Vec::with_capacity(1 + 4 + 4 + key.len());
        btree_key.push(0u8);
        btree_key.extend_from_slice(&node.to_be_bytes());
        btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
        btree_key.extend_from_slice(key.as_bytes());

        let encoded = value.encode();
        let blob_id = crate::blob_store::BlobStore::write(pager, &encoded).unwrap();
        tree.insert(pager, &btree_key, blob_id).unwrap();
    }

    fn insert_edge_prop(
        pager: &mut Pager,
        tree: &mut BTree,
        edge: nervusdb_api::EdgeKey,
        key: &str,
        value: PropertyValue,
    ) {
        let mut btree_key = Vec::with_capacity(1 + 4 + 4 + 4 + 4 + key.len());
        btree_key.push(1u8);
        btree_key.extend_from_slice(&edge.src.to_be_bytes());
        btree_key.extend_from_slice(&edge.rel.to_be_bytes());
        btree_key.extend_from_slice(&edge.dst.to_be_bytes());
        btree_key.extend_from_slice(&(key.len() as u32).to_be_bytes());
        btree_key.extend_from_slice(key.as_bytes());

        let encoded = value.encode();
        let blob_id = crate::blob_store::BlobStore::write(pager, &encoded).unwrap();
        tree.insert(pager, &btree_key, blob_id).unwrap();
    }

    #[test]
    fn read_node_property_from_store_reads_inserted_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ndb.bin");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        insert_node_prop(&mut pager, &mut tree, 7, "k", PropertyValue::Int(42));

        let root = tree.root().as_u64();
        let got = read_node_property_from_store(&pager, root, 7, "k");
        assert_eq!(got, Some(PropertyValue::Int(42)));
    }

    #[test]
    fn extend_node_properties_from_store_does_not_override_existing_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ndb.bin");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        insert_node_prop(&mut pager, &mut tree, 1, "a", PropertyValue::Int(999));
        insert_node_prop(
            &mut pager,
            &mut tree,
            1,
            "b",
            PropertyValue::String("x".to_string()),
        );

        let root = tree.root().as_u64();
        let mut props = BTreeMap::from([("a".to_string(), PropertyValue::Int(1))]);
        extend_node_properties_from_store(&pager, root, 1, &mut props).unwrap();

        assert_eq!(props.get("a"), Some(&PropertyValue::Int(1)));
        assert_eq!(
            props.get("b"),
            Some(&PropertyValue::String("x".to_string()))
        );
    }

    #[test]
    fn read_edge_property_from_store_reads_inserted_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ndb.bin");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        let edge = nervusdb_api::EdgeKey {
            src: 1,
            rel: 2,
            dst: 3,
        };
        insert_edge_prop(&mut pager, &mut tree, edge, "k", PropertyValue::Bool(true));

        let root = tree.root().as_u64();
        let got = read_edge_property_from_store(&pager, root, edge, "k");
        assert_eq!(got, Some(PropertyValue::Bool(true)));
    }

    #[test]
    fn extend_edge_properties_from_store_does_not_override_existing_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ndb.bin");
        let mut pager = Pager::open(&path).unwrap();
        let mut tree = BTree::create(&mut pager).unwrap();

        let edge = nervusdb_api::EdgeKey {
            src: 10,
            rel: 20,
            dst: 30,
        };
        insert_edge_prop(&mut pager, &mut tree, edge, "a", PropertyValue::Int(999));
        insert_edge_prop(
            &mut pager,
            &mut tree,
            edge,
            "b",
            PropertyValue::String("ok".to_string()),
        );

        let root = tree.root().as_u64();
        let mut props = BTreeMap::from([("a".to_string(), PropertyValue::Int(1))]);
        extend_edge_properties_from_store(&pager, root, edge, &mut props).unwrap();

        assert_eq!(props.get("a"), Some(&PropertyValue::Int(1)));
        assert_eq!(
            props.get("b"),
            Some(&PropertyValue::String("ok".to_string()))
        );
    }
}
