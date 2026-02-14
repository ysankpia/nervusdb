use super::{
    Error, ExternalId, InternalNodeId, LabelId, PropertyValue, RelTypeId, Result, WriteableGraph,
};
use nervusdb_storage::engine::WriteTxn as EngineWriteTxn;

impl<'a> WriteableGraph for EngineWriteTxn<'a> {
    fn create_node(
        &mut self,
        external_id: ExternalId,
        label_id: LabelId,
    ) -> Result<InternalNodeId> {
        EngineWriteTxn::create_node(self, external_id, label_id)
            .map_err(|e| Error::Other(e.to_string()))
    }

    fn add_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        EngineWriteTxn::add_node_label(self, node, label_id)
            .map_err(|e| Error::Other(e.to_string()))
    }

    fn remove_node_label(&mut self, node: InternalNodeId, label_id: LabelId) -> Result<()> {
        EngineWriteTxn::remove_node_label(self, node, label_id)
            .map_err(|e| Error::Other(e.to_string()))
    }

    fn create_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()> {
        EngineWriteTxn::create_edge(self, src, rel, dst);
        Ok(())
    }

    fn set_node_property(
        &mut self,
        node: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        EngineWriteTxn::set_node_property(self, node, key, value);
        Ok(())
    }

    fn set_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: String,
        value: PropertyValue,
    ) -> Result<()> {
        EngineWriteTxn::set_edge_property(self, src, rel, dst, key, value);
        Ok(())
    }

    fn remove_node_property(&mut self, node: InternalNodeId, key: &str) -> Result<()> {
        EngineWriteTxn::remove_node_property(self, node, key);
        Ok(())
    }

    fn remove_edge_property(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
        key: &str,
    ) -> Result<()> {
        EngineWriteTxn::remove_edge_property(self, src, rel, dst, key);
        Ok(())
    }

    fn tombstone_node(&mut self, node: InternalNodeId) -> Result<()> {
        EngineWriteTxn::tombstone_node(self, node);
        Ok(())
    }

    fn tombstone_edge(
        &mut self,
        src: InternalNodeId,
        rel: RelTypeId,
        dst: InternalNodeId,
    ) -> Result<()> {
        EngineWriteTxn::tombstone_edge(self, src, rel, dst);
        Ok(())
    }

    fn get_or_create_label_id(&mut self, name: &str) -> Result<LabelId> {
        EngineWriteTxn::get_or_create_label(self, name).map_err(|e| Error::Other(e.to_string()))
    }

    fn get_or_create_rel_type_id(&mut self, name: &str) -> Result<RelTypeId> {
        EngineWriteTxn::get_or_create_rel_type(self, name).map_err(|e| Error::Other(e.to_string()))
    }

    fn staged_created_nodes_with_labels(&self) -> Vec<(InternalNodeId, Vec<String>)> {
        EngineWriteTxn::staged_created_nodes_with_labels(self)
    }
}
