//! NervusDB core Rust library providing the low level storage primitives.

pub mod algorithms;
mod error;
#[cfg(not(target_arch = "wasm32"))]
pub mod ffi;
#[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
mod fts_index;
#[cfg(not(target_arch = "wasm32"))]
pub mod migration;
pub mod parser;
pub mod query;
pub mod storage;
pub mod triple;
#[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
mod vector_index;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use storage::{Hexastore, open_store};

pub type StringId = u64;
pub use error::{Error, Result};
#[cfg(not(target_arch = "wasm32"))]
use redb::{Database as RedbDatabase, WriteTransaction};
// Re-export Temporal Store types from nervusdb-temporal crate
#[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
pub use nervusdb_temporal::{
    EnsureEntityOptions, EpisodeInput, EpisodeLinkOptions, EpisodeLinkRecord, FactWriteInput,
    StoredAlias, StoredEntity, StoredEpisode, StoredFact, TemporalStoreV2 as TemporalStore,
    TimelineQuery, TimelineRole,
};
pub use triple::{Fact, Triple};

/// Database configuration used when opening an instance.
#[derive(Debug, Clone)]
pub struct Options {
    data_path: PathBuf,
}

impl Options {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            data_path: path.as_ref().to_owned(),
        }
    }
}

pub struct Database {
    store: Box<dyn Hexastore + Send>,
    #[cfg(not(target_arch = "wasm32"))]
    redb: Arc<RedbDatabase>,
    #[cfg(all(any(feature = "vector", feature = "fts"), not(target_arch = "wasm32")))]
    redb_path: PathBuf,
    #[cfg(not(target_arch = "wasm32"))]
    active_write: Option<WriteTransaction>,
    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    fts_index: Option<fts_index::FtsIndex>,
    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    fts_write_log: HashMap<u64, Vec<u8>>,
    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    vector_index: Option<vector_index::VectorIndex>,
    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    vector_undo_log: Vec<VectorUndoEntry>,
    #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
    temporal: TemporalStore,
    cursors: HashMap<u64, QueryCursor>,
    next_cursor_id: u64,
}

#[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
#[derive(Debug, Clone)]
struct VectorUndoEntry {
    node_id: u64,
    old: Option<Vec<f32>>,
}

struct QueryCursor {
    iter: crate::storage::HexastoreIter,
    finished: bool,
}

impl QueryCursor {
    fn new(iter: crate::storage::HexastoreIter) -> Self {
        Self {
            iter,
            finished: false,
        }
    }

    fn next_batch(&mut self, batch_size: usize) -> (Vec<Triple>, bool) {
        let mut batch = Vec::with_capacity(batch_size);
        for _ in 0..batch_size.max(1) {
            match self.iter.next() {
                Some(triple) => batch.push(triple),
                None => {
                    self.finished = true;
                    break;
                }
            }
        }
        let done = self.finished || batch.is_empty();
        (batch, done)
    }
}

fn debug_env_enabled() -> bool {
    match std::env::var("NERVUSDB_DEBUG_NATIVE") {
        Ok(val) => val == "1" || val.eq_ignore_ascii_case("true"),
        Err(_) => false,
    }
}

fn emit_debug(message: &str) {
    if debug_env_enabled() {
        eprintln!("{}", message);
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct QueryCriteria {
    pub subject_id: Option<StringId>,
    pub predicate_id: Option<StringId>,
    pub object_id: Option<StringId>,
}

impl Database {
    /// Opens a database at the specified path.
    ///
    /// The path should be a base path or directory.
    /// We will create: path.redb (single-file storage for triples, dictionary, temporal data)
    pub fn open(options: Options) -> Result<Self> {
        let path = options.data_path;
        // Ensure parent dir exists
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Other(e.to_string()))?;
        }

        #[cfg(not(target_arch = "wasm32"))]
        let redb_path = path.with_extension("redb");
        #[cfg(not(target_arch = "wasm32"))]
        let redb = Arc::new(
            RedbDatabase::create(redb_path.clone())
                .map_err(|e| Error::Other(format!("failed to open redb: {e}")))?,
        );

        #[cfg(not(target_arch = "wasm32"))]
        let store = open_store(redb.clone())?;
        #[cfg(target_arch = "wasm32")]
        let store = open_store(&path)?;

        #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
        let temporal = TemporalStore::open(redb.clone())?;

        #[cfg_attr(
            not(all(any(feature = "vector", feature = "fts"), not(target_arch = "wasm32"))),
            allow(unused_mut)
        )]
        let mut db = Self {
            store,
            #[cfg(not(target_arch = "wasm32"))]
            redb,
            #[cfg(all(any(feature = "vector", feature = "fts"), not(target_arch = "wasm32")))]
            redb_path: redb_path.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            active_write: None,
            #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
            fts_index: None,
            #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
            fts_write_log: HashMap::new(),
            #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
            vector_index: None,
            #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
            vector_undo_log: Vec::new(),
            #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
            temporal,
            cursors: HashMap::new(),
            next_cursor_id: 1,
        };

        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        {
            db.vector_index = vector_index::VectorIndex::open_or_rebuild(&db, &db.redb_path)?;
        }

        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        {
            db.fts_index = fts_index::FtsIndex::open_or_rebuild(&db, &db.redb_path)?;
        }

        Ok(db)
    }

    pub fn hydrate(
        &mut self,
        dictionary_values: Vec<String>,
        triples: Vec<(StringId, StringId, StringId)>,
    ) -> Result<()> {
        // Pre-intern dictionary values to ensure IDs match if they are sequential?
        // Wait, if we just intern, we get new IDs.
        // hydrate assumes specific IDs?
        // The signature is `triples: Vec<(StringId, StringId, StringId)>`. StringId is u64.
        // If the input triples use IDs that correspond to `dictionary_values` indices, we need to ensure that mapping.
        // But `intern` assigns IDs.
        // If `hydrate` is used for restoring a dump, the dump should probably contain string triples, not ID triples.
        // Or we assume `dictionary_values` are in ID order 1..N.
        // Let's assume `dictionary_values` are ordered by ID.

        // Note: This implementation assumes that `dictionary_values` index + 1 == ID.
        // This is fragile but matches the previous behavior where Dictionary assigned sequential IDs.
        for value in dictionary_values {
            self.store.intern(&value)?;
        }

        for (subject_id, predicate_id, object_id) in triples {
            let triple = Triple::new(subject_id, predicate_id, object_id);
            self.store.insert(&triple)?;
        }

        self.reset_cursors();

        Ok(())
    }

    /// Set node properties from JSON string (converts to FlexBuffers internally)
    pub fn set_node_property(&mut self, id: u64, json: &str) -> Result<()> {
        // Parse JSON and convert to FlexBuffers for unified storage
        let props: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(json)
                .map_err(|e| Error::Other(format!("Invalid JSON in set_node_property: {}", e)))?;
        let binary = crate::storage::property::serialize_properties(&props)?;
        self.set_node_property_binary(id, &binary)
    }

    /// Get node properties as JSON string (reads from FlexBuffers and converts)
    pub fn get_node_property(&self, id: u64) -> Result<Option<String>> {
        // Get binary data (FlexBuffers format)
        if let Some(binary) = self.get_node_property_binary(id)? {
            // Deserialize from FlexBuffers and convert to JSON
            let props = crate::storage::property::deserialize_properties(&binary)?;
            let json_string = serde_json::to_string(&props)
                .map_err(|e| Error::Other(format!("Failed to serialize to JSON: {}", e)))?;
            Ok(Some(json_string))
        } else {
            Ok(None)
        }
    }

    /// Set edge properties from JSON string (converts to FlexBuffers internally)
    pub fn set_edge_property(&mut self, s: u64, p: u64, o: u64, json: &str) -> Result<()> {
        // Parse JSON and convert to FlexBuffers for unified storage
        let props: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(json)
                .map_err(|e| Error::Other(format!("Invalid JSON in set_edge_property: {}", e)))?;
        let binary = crate::storage::property::serialize_properties(&props)?;
        self.set_edge_property_binary(s, p, o, &binary)
    }

    /// Get edge properties as JSON string (reads from FlexBuffers and converts)
    pub fn get_edge_property(&self, s: u64, p: u64, o: u64) -> Result<Option<String>> {
        // Get binary data (FlexBuffers format)
        if let Some(binary) = self.get_edge_property_binary(s, p, o)? {
            // Deserialize from FlexBuffers and convert to JSON
            let props = crate::storage::property::deserialize_properties(&binary)?;
            let json_string = serde_json::to_string(&props)
                .map_err(|e| Error::Other(format!("Failed to serialize to JSON: {}", e)))?;
            Ok(Some(json_string))
        } else {
            Ok(None)
        }
    }

    // Binary property methods (v2.0, FlexBuffers for 10x performance)

    pub fn set_node_property_binary(&mut self, id: u64, value: &[u8]) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(txn) = self.active_write.as_mut() {
            {
                let mut table = txn
                    .open_table(crate::storage::schema::TABLE_NODE_PROPS_BINARY)
                    .map_err(|e| Error::Other(e.to_string()))?;
                table
                    .insert(id, value)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
            fts_index::bump_committed_writes_in_txn(txn, 1)?;
        } else {
            let tx = self
                .redb
                .begin_write()
                .map_err(|e| Error::Other(e.to_string()))?;
            {
                let mut table = tx
                    .open_table(crate::storage::schema::TABLE_NODE_PROPS_BINARY)
                    .map_err(|e| Error::Other(e.to_string()))?;
                table
                    .insert(id, value)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
            fts_index::bump_committed_writes_in_txn(&tx, 1)?;

            tx.commit().map_err(|e| Error::Other(e.to_string()))?;
            self.store.after_write_commit();
        }

        #[cfg(target_arch = "wasm32")]
        self.store.set_node_property_binary(id, value)?;

        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        self.update_vector_index_from_node_props(id, value);

        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        self.update_fts_index_from_node_props(id, value);

        Ok(())
    }

    pub fn get_node_property_binary(&self, id: u64) -> Result<Option<Vec<u8>>> {
        self.store.get_node_property_binary(id)
    }

    pub fn set_edge_property_binary(&mut self, s: u64, p: u64, o: u64, value: &[u8]) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(txn) = self.active_write.as_mut() {
            let mut table = txn
                .open_table(crate::storage::schema::TABLE_EDGE_PROPS_BINARY)
                .map_err(|e| Error::Other(e.to_string()))?;
            table
                .insert((s, p, o), value)
                .map_err(|e| Error::Other(e.to_string()))?;
        } else {
            self.store.set_edge_property_binary(s, p, o, value)?;
        }

        Ok(())
    }

    pub fn get_edge_property_binary(&self, s: u64, p: u64, o: u64) -> Result<Option<Vec<u8>>> {
        self.store.get_edge_property_binary(s, p, o)
    }

    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    fn update_vector_index_from_node_props(&mut self, node_id: u64, value: &[u8]) {
        let Some(index) = self.vector_index.as_mut() else {
            return;
        };

        let Ok(props) = crate::storage::property::deserialize_properties(value) else {
            return;
        };

        if self.active_write.is_some() {
            let old = index.get_vector(node_id).ok().flatten();
            self.vector_undo_log.push(VectorUndoEntry { node_id, old });
        }

        if index.upsert_from_props(node_id, &props).is_err() {
            self.vector_index = None;
            self.vector_undo_log.clear();
        }
    }

    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    fn rollback_vector_index(&mut self) {
        let Some(index) = self.vector_index.as_mut() else {
            self.vector_undo_log.clear();
            return;
        };

        for entry in self.vector_undo_log.drain(..).rev() {
            let _ = index.upsert(entry.node_id, entry.old.as_deref());
        }
    }

    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    fn update_fts_index_from_node_props(&mut self, node_id: u64, value: &[u8]) {
        let Some(index) = self.fts_index.as_mut() else {
            return;
        };

        if self.active_write.is_some() {
            self.fts_write_log.insert(node_id, value.to_vec());
            return;
        }

        let Ok(props) = crate::storage::property::deserialize_properties(value) else {
            return;
        };
        if index.upsert_from_props(node_id, &props).is_err() {
            self.fts_index = None;
            self.fts_write_log.clear();
        }
    }

    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    pub(crate) fn fts_txt_score(&self, node_id: u64, property: &str, query: &str) -> f64 {
        let Some(index) = self.fts_index.as_ref() else {
            return 0.0;
        };
        index.txt_score(node_id, property, query).unwrap_or(0.0) as f64
    }

    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    pub(crate) fn fts_scores_for_query(
        &self,
        property: &str,
        query: &str,
    ) -> Option<Arc<HashMap<u64, f32>>> {
        let index = self.fts_index.as_ref()?;
        index.scores_for_query(property, query).ok()
    }

    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    pub fn configure_fts_index(&mut self, mode: &str) -> Result<()> {
        if self.active_write.is_some() {
            return Err(Error::Other(
                "cannot configure fts index during active transaction".to_string(),
            ));
        }

        let config = fts_index::FtsIndexConfig {
            mode: if mode.is_empty() {
                "all_string_props".to_string()
            } else {
                mode.to_string()
            },
        };
        fts_index::write_config(self, &config)?;
        self.fts_index = fts_index::FtsIndex::open_or_rebuild(self, &self.redb_path)?;
        self.fts_write_log.clear();
        Ok(())
    }

    #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
    pub fn disable_fts_index(&mut self) -> Result<()> {
        if self.active_write.is_some() {
            return Err(Error::Other(
                "cannot disable fts index during active transaction".to_string(),
            ));
        }
        fts_index::clear_config(self)?;
        self.fts_index = None;
        self.fts_write_log.clear();
        Ok(())
    }

    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    pub fn configure_vector_index(
        &mut self,
        dim: usize,
        property: &str,
        metric: &str,
    ) -> Result<()> {
        if self.active_write.is_some() {
            return Err(Error::Other(
                "cannot configure vector index during active transaction".to_string(),
            ));
        }
        if dim == 0 {
            return Err(Error::Other("vector dim must be > 0".to_string()));
        }

        let config = vector_index::VectorIndexConfig {
            dim,
            property: property.to_string(),
            metric: metric.to_string(),
        };
        vector_index::write_config(self, &config)?;
        self.vector_index = vector_index::VectorIndex::open_or_rebuild(self, &self.redb_path)?;
        Ok(())
    }

    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    pub fn disable_vector_index(&mut self) -> Result<()> {
        if self.active_write.is_some() {
            return Err(Error::Other(
                "cannot disable vector index during active transaction".to_string(),
            ));
        }
        vector_index::clear_config(self)?;
        self.vector_index = None;
        self.vector_undo_log.clear();
        Ok(())
    }

    #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
    pub fn vector_search(&self, query: &[f32], limit: usize) -> Result<Vec<(u64, f32)>> {
        let Some(index) = self.vector_index.as_ref() else {
            return Err(Error::Other("vector index not configured".to_string()));
        };
        index.search(query, limit)
    }

    pub fn flush_indexes(&mut self) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        if self.active_write.is_some() {
            return Err(Error::Other(
                "cannot flush indexes during active transaction".to_string(),
            ));
        }

        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        if let Some(index) = self.vector_index.as_mut() {
            index.flush()?;
        }

        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        {
            let committed_writes = fts_index::read_committed_writes(self)?;
            if let Some(index) = self.fts_index.as_mut() {
                index.flush(committed_writes)?;
            }
        }

        Ok(())
    }

    // Batch operations (v2.0, for migration and bulk operations)

    /// Insert multiple triples in a single transaction
    /// Returns the number of triples actually inserted (excludes duplicates)
    pub fn batch_insert(&mut self, triples: &[Triple]) -> Result<usize> {
        self.store.batch_insert(triples)
    }

    /// Delete multiple triples in a single transaction
    /// Returns the number of triples actually deleted
    pub fn batch_delete(&mut self, triples: &[Triple]) -> Result<usize> {
        self.store.batch_delete(triples)
    }

    /// Insert multiple facts in a single optimized transaction
    /// Uses cached table handles for maximum performance
    /// Returns the triples that were inserted
    pub fn batch_add_facts(&mut self, facts: &[Fact<'_>]) -> Result<Vec<Triple>> {
        self.store.batch_insert_facts(facts)
    }

    pub fn add_fact(&mut self, fact: Fact<'_>) -> Result<Triple> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(txn) = self.active_write.as_mut() {
            let s = crate::storage::disk::intern_in_txn(txn, fact.subject)?;
            let p = crate::storage::disk::intern_in_txn(txn, fact.predicate)?;
            let o = crate::storage::disk::intern_in_txn(txn, fact.object)?;
            let triple = Triple::new(s, p, o);
            crate::storage::disk::insert_triple(txn, &triple)?;
            return Ok(triple);
        }
        self.store.insert_fact(fact)
    }

    pub fn delete_fact(&mut self, fact: Fact<'_>) -> Result<bool> {
        let s = self.resolve_id(fact.subject)?.ok_or(Error::NotFound)?;
        let p = self.resolve_id(fact.predicate)?.ok_or(Error::NotFound)?;
        let o = self.resolve_id(fact.object)?.ok_or(Error::NotFound)?;
        let triple = Triple::new(s, p, o);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(txn) = self.active_write.as_mut() {
            return crate::storage::disk::delete_triple(txn, &triple);
        }
        self.store.delete(&triple)
    }

    pub fn all_triples(&self) -> Vec<Triple> {
        self.store.iter().collect()
    }

    pub fn resolve_str(&self, id: StringId) -> Result<Option<String>> {
        self.store.resolve_str(id)
    }

    pub fn resolve_id(&self, value: &str) -> Result<Option<StringId>> {
        self.store.resolve_id(value)
    }

    pub fn intern(&mut self, value: &str) -> Result<u64> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(txn) = self.active_write.as_mut() {
            return crate::storage::disk::intern_in_txn(txn, value);
        }
        self.store.intern(value)
    }

    /// Bulk intern strings in a single transaction (order preserving).
    pub fn bulk_intern(&mut self, values: &[&str]) -> Result<Vec<u64>> {
        self.store.bulk_intern(values)
    }

    pub fn dictionary_size(&self) -> Result<u64> {
        self.store.dictionary_size()
    }

    pub fn execute_query(
        &mut self,
        query_string: &str,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        self.execute_query_with_params(query_string, None)
    }

    pub fn execute_query_with_params(
        &mut self,
        query_string: &str,
        params: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::parser::Parser;

        let query = Parser::parse(query_string)?;

        let param_values: HashMap<String, query::executor::Value> = params
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, Self::serde_value_to_executor_value(v)))
            .collect();

        if debug_env_enabled() {
            let keys: Vec<_> = param_values.keys().cloned().collect();
            emit_debug(&format!(
                "[nervusdb-core] execute_query_with_params received: {:?}",
                keys
            ));
        }

        self.execute_parsed_query_with_params(query, &param_values)
    }

    fn execute_parsed_query_with_params(
        &mut self,
        query: query::ast::Query,
        param_values: &HashMap<String, query::executor::Value>,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::ast::Clause;
        use query::executor::{ExecutionContext, ExecutionPlan};
        use query::planner::QueryPlanner;

        // Handle CALL { ... } queries directly (simplified: only standalone CALL for MVP).
        if query.clauses.len() == 1
            && let Clause::Call(call_clause) = &query.clauses[0]
        {
            return self.execute_parsed_query_with_params(call_clause.query.clone(), param_values);
        }
        if query
            .clauses
            .iter()
            .any(|clause| matches!(clause, Clause::Call(_)))
        {
            return Err(Error::NotImplemented("CALL with other clauses"));
        }
        if query
            .clauses
            .iter()
            .any(|clause| matches!(clause, Clause::Union(_)))
        {
            return self.execute_union_query_with_params(query, param_values);
        }

        // Check if query contains SET clause (needs special handling due to mutation)
        let has_set = query
            .clauses
            .iter()
            .any(|clause| matches!(clause, Clause::Set(_)));

        // Check if query contains DELETE clause (needs special handling due to mutation)
        let has_delete = query
            .clauses
            .iter()
            .any(|clause| matches!(clause, Clause::Delete(_)));

        // Handle CREATE queries directly (simplified: only standalone CREATE for MVP)
        if query.clauses.len() == 1
            && let Clause::Create(create_clause) = &query.clauses[0]
        {
            // Execute CREATE immediately
            return self.execute_create_pattern(&create_clause.pattern);
        }

        // Handle MERGE queries directly (simplified: only standalone MERGE for MVP)
        if query.clauses.len() == 1
            && let Clause::Merge(merge_clause) = &query.clauses[0]
        {
            return self.execute_merge_pattern(&merge_clause.pattern);
        }

        // Handle SET queries specially (needs mutation)
        if has_set {
            return self.execute_set_query_with_plan(&query, param_values);
        }

        // Handle DELETE queries specially (needs mutation)
        if has_delete {
            return self.execute_delete_query_with_plan(&query, param_values);
        }

        // Handle read queries through execution plan
        let planner = QueryPlanner::new();
        let plan = planner.plan(query)?;

        let ctx = ExecutionContext {
            db: self,
            params: param_values,
        };
        let iterator = plan.execute(&ctx)?;

        let mut results = Vec::new();
        for record in iterator {
            results.push(record?.values);
        }

        Ok(results)
    }

    fn execute_union_query_with_params(
        &mut self,
        query: query::ast::Query,
        param_values: &HashMap<String, query::executor::Value>,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::ast::{Clause, Expression, Query, ReturnClause, UnionClause};

        fn is_write_clause(clause: &Clause) -> bool {
            matches!(
                clause,
                Clause::Create(_) | Clause::Merge(_) | Clause::Set(_) | Clause::Delete(_)
            )
        }

        fn infer_alias(expr: &Expression) -> String {
            match expr {
                Expression::Variable(name) => name.clone(),
                Expression::PropertyAccess(pa) => format!("{}.{}", pa.variable, pa.property),
                _ => "col".to_string(),
            }
        }

        fn return_columns(query: &Query) -> Option<Vec<String>> {
            query.clauses.iter().find_map(|clause| match clause {
                Clause::Return(ReturnClause { items, .. }) => Some(
                    items
                        .iter()
                        .map(|item| {
                            item.alias
                                .clone()
                                .unwrap_or_else(|| infer_alias(&item.expression))
                        })
                        .collect(),
                ),
                _ => None,
            })
        }

        fn validate_row_columns(
            expected: &[String],
            row: &std::collections::HashMap<String, query::executor::Value>,
        ) -> Result<()> {
            if row.len() != expected.len() {
                return Err(Error::Other("UNION schema mismatch".to_string()));
            }
            for col in expected {
                if !row.contains_key(col) {
                    return Err(Error::Other("UNION schema mismatch".to_string()));
                }
            }
            Ok(())
        }

        fn row_key(row: &std::collections::HashMap<String, query::executor::Value>) -> String {
            let mut items: Vec<_> = row.iter().collect();
            items.sort_by(|(a, _), (b, _)| a.cmp(b));
            items
                .into_iter()
                .map(|(k, v)| format!("{k}={v:?}"))
                .collect::<Vec<_>>()
                .join("|")
        }

        let mut left_clauses = Vec::new();
        let mut unions: Vec<UnionClause> = Vec::new();

        for clause in query.clauses {
            match clause {
                Clause::Union(u) => unions.push(u),
                other => left_clauses.push(other),
            }
        }

        let left_query = Query {
            clauses: left_clauses,
        };

        if left_query.clauses.iter().any(is_write_clause)
            || unions
                .iter()
                .any(|u| u.query.clauses.iter().any(is_write_clause))
        {
            return Err(Error::NotImplemented("UNION with write clauses"));
        }

        let Some(expected_cols) = return_columns(&left_query) else {
            return Err(Error::Other("UNION requires explicit RETURN".to_string()));
        };

        for u in &unions {
            let Some(cols) = return_columns(&u.query) else {
                return Err(Error::Other("UNION requires explicit RETURN".to_string()));
            };
            if cols != expected_cols {
                return Err(Error::Other(
                    "UNION queries must return the same columns".to_string(),
                ));
            }
        }

        let mut current = self.execute_parsed_query_with_params(left_query, param_values)?;
        for row in &current {
            validate_row_columns(&expected_cols, row)?;
        }

        for u in unions {
            let mut right = self.execute_parsed_query_with_params(u.query, param_values)?;
            for row in &right {
                validate_row_columns(&expected_cols, row)?;
            }

            if u.all {
                current.append(&mut right);
            } else {
                let mut deduped = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for row in current.into_iter().chain(right) {
                    if seen.insert(row_key(&row)) {
                        deduped.push(row);
                    }
                }
                current = deduped;
            }
        }

        Ok(current)
    }

    /// Convert serde_json::Value to executor::Value (public for FFI)
    pub fn serde_value_to_executor_value(value: serde_json::Value) -> query::executor::Value {
        use query::executor::Value as ExecValue;

        match value {
            serde_json::Value::String(s) => ExecValue::String(s),
            serde_json::Value::Number(n) => ExecValue::Float(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::Bool(b) => ExecValue::Boolean(b),
            serde_json::Value::Null => ExecValue::Null,
            serde_json::Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in &items {
                    let Some(n) = item.as_f64() else {
                        return ExecValue::String(serde_json::Value::Array(items).to_string());
                    };
                    out.push(n as f32);
                }
                ExecValue::Vector(out)
            }
            _ => ExecValue::Null,
        }
    }

    fn execute_create_pattern(
        &mut self,
        pattern: &query::ast::Pattern,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::ast::{PathElement, RelationshipDirection};
        use query::executor::Value;
        use std::collections::HashMap;

        let mut result_record: HashMap<String, Value> = HashMap::new();
        let mut last_node_info: Option<(String, u64)> = None;

        let mut i = 0;
        while i < pattern.elements.len() {
            match &pattern.elements[i] {
                PathElement::Node(node_pattern) => {
                    // Create a node by adding a type triple
                    let anon_name = format!("_anon{}", i);
                    let node_str = node_pattern.variable.as_deref().unwrap_or(&anon_name);
                    let label = node_pattern
                        .labels
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("Node");

                    let fact = self.add_fact(Fact::new(node_str, "type", label))?;
                    let node_id = fact.subject_id;

                    // Set node properties if specified
                    if let Some(props) = &node_pattern.properties {
                        let props_map = self.convert_property_map_to_json(props)?;
                        let binary = crate::storage::property::serialize_properties(&props_map)?;
                        self.set_node_property_binary(node_id, &binary)?;
                    }

                    if let Some(var) = &node_pattern.variable {
                        result_record.insert(var.clone(), Value::Node(node_id));
                        last_node_info = Some((var.clone(), node_id));
                    } else {
                        last_node_info = Some((format!("_anon{}", i), node_id));
                    }
                }
                PathElement::Relationship(rel_pattern) => {
                    // Relationship must be between two nodes: (a)-[r]->(b)
                    // We just processed a node, now expect another node after this relationship
                    if i + 1 >= pattern.elements.len() {
                        return Err(Error::Other(
                            "Relationship must be followed by a node".to_string(),
                        ));
                    }

                    if let Some((_, start_node_id)) = last_node_info {
                        // Process the next node (end node)
                        i += 1;
                        if let PathElement::Node(end_node_pattern) = &pattern.elements[i] {
                            let end_anon_name = format!("_anon{}", i);
                            let end_node_str = end_node_pattern
                                .variable
                                .as_deref()
                                .unwrap_or(&end_anon_name);
                            let end_label = end_node_pattern
                                .labels
                                .first()
                                .map(|s| s.as_str())
                                .unwrap_or("Node");

                            let end_fact =
                                self.add_fact(Fact::new(end_node_str, "type", end_label))?;
                            let end_node_id = end_fact.subject_id;

                            // Set end node properties
                            if let Some(props) = &end_node_pattern.properties {
                                let props_map = self.convert_property_map_to_json(props)?;
                                let binary =
                                    crate::storage::property::serialize_properties(&props_map)?;
                                self.set_node_property_binary(end_node_id, &binary)?;
                            }

                            if let Some(var) = &end_node_pattern.variable {
                                result_record.insert(var.clone(), Value::Node(end_node_id));
                            }

                            // Create the relationship triple
                            let rel_type = rel_pattern
                                .types
                                .first()
                                .map(|s| s.as_str())
                                .unwrap_or("RELATED_TO");

                            // Determine direction
                            let (subject_id, object_id) = match rel_pattern.direction {
                                RelationshipDirection::LeftToRight => (start_node_id, end_node_id),
                                RelationshipDirection::RightToLeft => (end_node_id, start_node_id),
                                RelationshipDirection::Undirected => (start_node_id, end_node_id), // Default to left-to-right
                            };

                            let subject_str = self.resolve_str(subject_id)?.ok_or_else(|| {
                                Error::Other("Subject node not found".to_string())
                            })?;
                            let object_str = self
                                .resolve_str(object_id)?
                                .ok_or_else(|| Error::Other("Object node not found".to_string()))?;

                            let rel_fact =
                                self.add_fact(Fact::new(&subject_str, rel_type, &object_str))?;

                            // Set relationship properties if specified
                            if let Some(props) = &rel_pattern.properties {
                                let props_map = self.convert_property_map_to_json(props)?;
                                let binary =
                                    crate::storage::property::serialize_properties(&props_map)?;
                                self.set_edge_property_binary(
                                    rel_fact.subject_id,
                                    rel_fact.predicate_id,
                                    rel_fact.object_id,
                                    &binary,
                                )?;
                            }

                            // Store relationship in result if it has a variable
                            if let Some(var) = &rel_pattern.variable {
                                result_record.insert(var.clone(), Value::Relationship(rel_fact));
                            }

                            // Update last node info for potential next relationship
                            last_node_info = end_node_pattern
                                .variable
                                .as_ref()
                                .map(|v| (v.clone(), end_node_id))
                                .or(Some((format!("_anon{}", i), end_node_id)));
                        } else {
                            return Err(Error::Other(
                                "Expected node after relationship".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::Other("Relationship must follow a node".to_string()));
                    }
                }
            }
            i += 1;
        }

        Ok(vec![result_record])
    }

    fn execute_merge_pattern(
        &mut self,
        pattern: &query::ast::Pattern,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::ast::{PathElement, RelationshipDirection};
        use query::executor::Value;
        use std::collections::HashMap;

        let mut result_record: HashMap<String, Value> = HashMap::new();
        let mut last_node_info: Option<(String, u64)> = None;

        let mut i = 0;
        while i < pattern.elements.len() {
            match &pattern.elements[i] {
                PathElement::Node(node_pattern) => {
                    let anon_name = format!("_anon{}", i);
                    let node_str = node_pattern.variable.as_deref().unwrap_or(&anon_name);
                    let label = node_pattern
                        .labels
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("Node");

                    let node_id = self.ensure_node(node_str, label)?;

                    // Upsert node properties if specified.
                    if let Some(props) = &node_pattern.properties {
                        let props_map = self.convert_property_map_to_json(props)?;
                        let binary = crate::storage::property::serialize_properties(&props_map)?;
                        self.set_node_property_binary(node_id, &binary)?;
                    }

                    if let Some(var) = &node_pattern.variable {
                        result_record.insert(var.clone(), Value::Node(node_id));
                        last_node_info = Some((var.clone(), node_id));
                    } else {
                        last_node_info = Some((anon_name, node_id));
                    }
                }
                PathElement::Relationship(rel_pattern) => {
                    if i + 1 >= pattern.elements.len() {
                        return Err(Error::Other(
                            "Relationship must be followed by a node".to_string(),
                        ));
                    }

                    let Some((_, start_node_id)) = last_node_info else {
                        return Err(Error::Other("Relationship must follow a node".to_string()));
                    };

                    i += 1;
                    let PathElement::Node(end_node_pattern) = &pattern.elements[i] else {
                        return Err(Error::Other("Expected node after relationship".to_string()));
                    };

                    let end_anon_name = format!("_anon{}", i);
                    let end_node_str = end_node_pattern
                        .variable
                        .as_deref()
                        .unwrap_or(&end_anon_name);
                    let end_label = end_node_pattern
                        .labels
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("Node");

                    let end_node_id = self.ensure_node(end_node_str, end_label)?;

                    // Upsert end node properties
                    if let Some(props) = &end_node_pattern.properties {
                        let props_map = self.convert_property_map_to_json(props)?;
                        let binary = crate::storage::property::serialize_properties(&props_map)?;
                        self.set_node_property_binary(end_node_id, &binary)?;
                    }

                    if let Some(var) = &end_node_pattern.variable {
                        result_record.insert(var.clone(), Value::Node(end_node_id));
                    }

                    let rel_type = rel_pattern
                        .types
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("RELATED_TO");

                    let (subject_id, object_id) = match rel_pattern.direction {
                        RelationshipDirection::LeftToRight => (start_node_id, end_node_id),
                        RelationshipDirection::RightToLeft => (end_node_id, start_node_id),
                        RelationshipDirection::Undirected => (start_node_id, end_node_id),
                    };

                    let rel_triple = self.ensure_relationship(subject_id, rel_type, object_id)?;

                    // Upsert relationship properties if specified.
                    if let Some(props) = &rel_pattern.properties {
                        let props_map = self.convert_property_map_to_json(props)?;
                        let binary = crate::storage::property::serialize_properties(&props_map)?;
                        self.set_edge_property_binary(
                            rel_triple.subject_id,
                            rel_triple.predicate_id,
                            rel_triple.object_id,
                            &binary,
                        )?;
                    }

                    if let Some(var) = &rel_pattern.variable {
                        result_record.insert(var.clone(), Value::Relationship(rel_triple));
                    }

                    last_node_info = end_node_pattern
                        .variable
                        .as_ref()
                        .map(|v| (v.clone(), end_node_id))
                        .or(Some((end_anon_name, end_node_id)));
                }
            }
            i += 1;
        }

        Ok(vec![result_record])
    }

    fn ensure_node(&mut self, node_str: &str, label: &str) -> Result<u64> {
        let node_id = self.resolve_id(node_str)?;
        let type_id = self.resolve_id("type")?;
        let label_id = self.resolve_id(label)?;

        if let (Some(node_id), Some(type_id), Some(label_id)) = (node_id, type_id, label_id) {
            let criteria = QueryCriteria {
                subject_id: Some(node_id),
                predicate_id: Some(type_id),
                object_id: Some(label_id),
            };
            if self.query(criteria).next().is_some() {
                return Ok(node_id);
            }
        }

        let fact = self.add_fact(Fact::new(node_str, "type", label))?;
        Ok(fact.subject_id)
    }

    fn ensure_relationship(
        &mut self,
        subject_id: u64,
        rel_type: &str,
        object_id: u64,
    ) -> Result<Triple> {
        if let Some(predicate_id) = self.resolve_id(rel_type)? {
            let criteria = QueryCriteria {
                subject_id: Some(subject_id),
                predicate_id: Some(predicate_id),
                object_id: Some(object_id),
            };
            if self.query(criteria).next().is_some() {
                return Ok(Triple::new(subject_id, predicate_id, object_id));
            }
        }

        let subject_str = self
            .resolve_str(subject_id)?
            .ok_or_else(|| Error::Other("Subject node not found".to_string()))?;
        let object_str = self
            .resolve_str(object_id)?
            .ok_or_else(|| Error::Other("Object node not found".to_string()))?;

        self.add_fact(Fact::new(&subject_str, rel_type, &object_str))
    }

    fn convert_property_map_to_json(
        &self,
        prop_map: &query::ast::PropertyMap,
    ) -> Result<HashMap<String, serde_json::Value>> {
        use query::ast::{Expression, Literal};
        let mut map = HashMap::new();

        for pair in &prop_map.properties {
            let value = match &pair.value {
                Expression::Literal(lit) => match lit {
                    Literal::String(s) => serde_json::Value::String(s.clone()),
                    Literal::Float(f) => serde_json::json!(f),
                    Literal::Boolean(b) => serde_json::Value::Bool(*b),
                    Literal::Null => serde_json::Value::Null,
                    _ => serde_json::Value::Null,
                },
                _ => serde_json::Value::Null, // TODO: Support computed properties
            };
            map.insert(pair.key.clone(), value);
        }

        Ok(map)
    }

    fn execute_set_query_with_plan(
        &mut self,
        query: &query::ast::Query,
        params: &HashMap<String, query::executor::Value>,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::executor::{ExecutionContext, ExecutionPlan, Value, evaluate_expression_value};
        use query::planner::QueryPlanner;
        use std::collections::HashMap;

        // Build the execution plan
        let planner = QueryPlanner::new();
        let plan = planner.plan(query.clone())?;

        // Extract SetNode and optional ReturnClause from the plan
        let (set_node, return_clause) = self.extract_set_node(&plan, query)?;

        // Execute input plan to get all matching records
        let mut records: Vec<query::executor::Record> = {
            let ctx = ExecutionContext { db: &*self, params };
            let iterator = set_node.input.execute(&ctx)?;
            let mut rows = Vec::new();
            for record in iterator {
                rows.push(record?);
            }
            rows
        };

        // Now we can modify the database (no more borrowing conflict)
        for record in &mut records {
            // Apply each SET item
            for set_item in &set_node.items {
                let var_name = &set_item.property.variable;

                // Get the node ID from the record
                if let Some(Value::Node(node_id)) = record.get(var_name) {
                    let node_id = *node_id;

                    // Evaluate the new value expression
                    let new_value = {
                        let ctx = ExecutionContext { db: &*self, params };
                        evaluate_expression_value(&set_item.value, record, &ctx)
                    };

                    // Read existing properties
                    let mut props = if let Ok(Some(binary)) = self.get_node_property_binary(node_id)
                    {
                        crate::storage::property::deserialize_properties(&binary)?
                    } else {
                        HashMap::new()
                    };

                    // Update the property
                    let json_value = match new_value {
                        Value::String(s) => serde_json::Value::String(s),
                        Value::Float(f) => serde_json::json!(f),
                        Value::Boolean(b) => serde_json::Value::Bool(b),
                        Value::Null => serde_json::Value::Null,
                        _ => serde_json::Value::Null,
                    };
                    props.insert(set_item.property.property.clone(), json_value);

                    // Write back to database
                    let binary = crate::storage::property::serialize_properties(&props)?;
                    self.set_node_property_binary(node_id, &binary)?;
                }
            }
        }

        // Apply RETURN projection if present
        if let Some(return_clause) = return_clause {
            let mut results = Vec::new();
            for record in records {
                let mut result = HashMap::new();
                for item in &return_clause.items {
                    let alias = item
                        .alias
                        .clone()
                        .unwrap_or_else(|| match &item.expression {
                            query::ast::Expression::Variable(name) => name.clone(),
                            query::ast::Expression::PropertyAccess(pa) => {
                                format!("{}.{}", pa.variable, pa.property)
                            }
                            _ => "col".to_string(),
                        });
                    let value = {
                        let ctx = ExecutionContext { db: &*self, params };
                        evaluate_expression_value(&item.expression, &record, &ctx)
                    };
                    result.insert(alias, value);
                }
                results.push(result);
            }
            Ok(results)
        } else {
            // No RETURN clause, just return the records as-is
            Ok(records.into_iter().map(|r| r.values).collect())
        }
    }

    fn extract_set_node<'a>(
        &self,
        plan: &'a query::planner::PhysicalPlan,
        query: &query::ast::Query,
    ) -> Result<(
        &'a query::planner::SetNode,
        Option<query::ast::ReturnClause>,
    )> {
        // Find SET clause and RETURN clause
        let _set_clause = query
            .clauses
            .iter()
            .find_map(|c| {
                if let query::ast::Clause::Set(s) = c {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::Other("No SET clause found".to_string()))?;

        let return_clause = query.clauses.iter().find_map(|c| {
            if let query::ast::Clause::Return(r) = c {
                Some(r.clone())
            } else {
                None
            }
        });

        // Extract SetNode from plan (may be wrapped in Project/Filter)
        fn find_set_node(plan: &query::planner::PhysicalPlan) -> Option<&query::planner::SetNode> {
            match plan {
                query::planner::PhysicalPlan::Set(node) => Some(node),
                query::planner::PhysicalPlan::Project(node) => find_set_node(&node.input),
                query::planner::PhysicalPlan::Filter(node) => find_set_node(&node.input),
                _ => None,
            }
        }

        let set_node = find_set_node(plan)
            .ok_or_else(|| Error::Other("No SetNode found in plan".to_string()))?;

        Ok((set_node, return_clause))
    }

    fn execute_delete_query_with_plan(
        &mut self,
        query: &query::ast::Query,
        params: &HashMap<String, query::executor::Value>,
    ) -> Result<Vec<std::collections::HashMap<String, query::executor::Value>>> {
        use query::executor::{ExecutionContext, ExecutionPlan, Value, evaluate_expression_value};
        use query::planner::{PhysicalPlan, QueryPlanner};

        // Build the execution plan
        let planner = QueryPlanner::new();
        let plan = planner.plan(query.clone())?;

        // Extract DeleteNode and determine the correct input plan to execute
        // The plan might be Filter(Delete(Scan)), so we need to build Filter(Scan)
        let (delete_node, input_plan) = self.extract_delete_and_input_plan(&plan, query)?;

        // Build the actual plan to execute (without the Delete in the middle)
        let exec_plan: &PhysicalPlan = match &plan {
            PhysicalPlan::Delete(_) => {
                // Simple case: just Delete(input), execute the input directly
                &delete_node.input
            }
            PhysicalPlan::Filter(_filter_node) => {
                // Filter(Delete(input)) - we need to execute Filter(input)
                // But we can't modify the plan tree here, so just use input_plan
                // which is the entire Filter(Delete(...)) - NO that's wrong!
                //
                // Actually, we need to reconstruct Filter(delete_node.input)
                // But we can't easily do that because FilterNode contains Box<PhysicalPlan>
                //
                // Simpler solution: execute delete_node.input and manually apply the filter
                &delete_node.input
            }
            _ => input_plan,
        };

        // Execute input plan to get all matching records
        let base_records: Vec<query::executor::Record> = {
            let ctx = ExecutionContext { db: &*self, params };
            let iterator = exec_plan.execute(&ctx)?;
            let mut rows = Vec::new();
            for record in iterator {
                rows.push(record?);
            }
            rows
        };

        // If we have a Filter wrapping the Delete, we need to apply it manually
        // Extract the filter predicate from the plan
        let filter_predicate = if let PhysicalPlan::Filter(filter_node) = &plan {
            Some(&filter_node.predicate)
        } else {
            None
        };

        let mut records: Vec<query::executor::Record> = Vec::new();
        for rec in base_records {
            // Apply filter if present
            if let Some(predicate) = filter_predicate {
                use query::executor::evaluate_expression_value;
                let filter_result = {
                    let ctx = ExecutionContext { db: &*self, params };
                    evaluate_expression_value(predicate, &rec, &ctx)
                };
                if filter_result == Value::Boolean(true) {
                    records.push(rec);
                }
            } else {
                records.push(rec);
            }
        }

        // Collect all node IDs to delete
        let mut node_ids_to_delete = Vec::new();

        for record in &records {
            for expr in &delete_node.expressions {
                let value = {
                    let ctx = ExecutionContext { db: &*self, params };
                    evaluate_expression_value(expr, record, &ctx)
                };
                if let Value::Node(node_id) = value {
                    node_ids_to_delete.push(node_id);
                }
            }
        }

        // Now perform the deletion (no more borrowing conflict)
        for node_id in node_ids_to_delete {
            self.delete_node(node_id, delete_node.detach)?;
        }

        // DELETE doesn't return anything by default (Neo4j-style)
        Ok(Vec::new())
    }

    fn extract_delete_and_input_plan<'a>(
        &self,
        plan: &'a query::planner::PhysicalPlan,
        query: &query::ast::Query,
    ) -> Result<(
        &'a query::planner::DeleteNode,
        &'a query::planner::PhysicalPlan,
    )> {
        // Find DELETE clause
        let _delete_clause = query
            .clauses
            .iter()
            .find_map(|c| {
                if let query::ast::Clause::Delete(d) = c {
                    Some(d.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::Other("No DELETE clause found".to_string()))?;

        // The plan structure can be:
        // - Delete(input) - simple case
        // - Filter(Delete(input)) - with WHERE clause
        // - Project(Delete(input)) or Project(Filter(Delete(input))) - with RETURN clause

        // We want to return (DeleteNode, input_plan_with_filters)
        // The input_plan_with_filters should include any Filter that wraps the Delete

        match plan {
            query::planner::PhysicalPlan::Delete(delete_node) => {
                // Simple case: no Filter wrapping
                Ok((delete_node, &delete_node.input))
            }
            query::planner::PhysicalPlan::Filter(filter_node) => {
                // Filter wraps Delete
                match &*filter_node.input {
                    query::planner::PhysicalPlan::Delete(delete_node) => {
                        // Return DeleteNode and the Filter as the input plan
                        Ok((delete_node, plan))
                    }
                    _ => Err(Error::Other("Expected Delete inside Filter".to_string())),
                }
            }
            query::planner::PhysicalPlan::Project(project_node) => {
                // Project wraps Delete or Filter(Delete)
                match &*project_node.input {
                    query::planner::PhysicalPlan::Delete(delete_node) => {
                        Ok((delete_node, &delete_node.input))
                    }
                    query::planner::PhysicalPlan::Filter(filter_node) => {
                        match &*filter_node.input {
                            query::planner::PhysicalPlan::Delete(delete_node) => {
                                Ok((delete_node, &project_node.input))
                            }
                            _ => Err(Error::Other("Expected Delete inside Filter".to_string())),
                        }
                    }
                    _ => Err(Error::Other(
                        "Expected Delete or Filter(Delete) inside Project".to_string(),
                    )),
                }
            }
            _ => Err(Error::Other("No DELETE plan found".to_string())),
        }
    }

    fn delete_node(&mut self, node_id: u64, detach: bool) -> Result<()> {
        // First check if node has any relationships
        let has_relationships = self.node_has_relationships(node_id);

        if has_relationships && !detach {
            return Err(Error::Other(format!(
                "Cannot delete node {} because it has relationships. Use DETACH DELETE to remove relationships first.",
                node_id
            )));
        }

        // If DETACH, delete all relationships first
        if detach {
            self.delete_all_relationships(node_id)?;
        }

        // Delete node type metadata (triples where subject is node_id and predicate is "type")
        if let Some(type_id) = self.resolve_id("type")? {
            let criteria = QueryCriteria {
                subject_id: Some(node_id),
                predicate_id: Some(type_id),
                object_id: None,
            };

            let triples_to_delete: Vec<Triple> = self.query(criteria).collect();
            for triple in triples_to_delete {
                self.store.delete(&triple)?;
            }
        }

        // Delete node properties
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(txn) = self.active_write.as_mut() {
            {
                // Delete from binary table (v2.0)
                let mut binary_table = txn
                    .open_table(crate::storage::schema::TABLE_NODE_PROPS_BINARY)
                    .map_err(|e| Error::Other(e.to_string()))?;
                binary_table
                    .remove(node_id)
                    .map_err(|e| Error::Other(e.to_string()))?;

                // Delete from legacy string table (v1.x)
                let mut string_table = txn
                    .open_table(crate::storage::schema::TABLE_NODE_PROPS)
                    .map_err(|e| Error::Other(e.to_string()))?;
                string_table
                    .remove(node_id)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
            fts_index::bump_committed_writes_in_txn(txn, 1)?;
        } else {
            let tx = self
                .redb
                .begin_write()
                .map_err(|e| Error::Other(e.to_string()))?;
            {
                // Delete from binary table (v2.0)
                let mut binary_table = tx
                    .open_table(crate::storage::schema::TABLE_NODE_PROPS_BINARY)
                    .map_err(|e| Error::Other(e.to_string()))?;
                binary_table
                    .remove(node_id)
                    .map_err(|e| Error::Other(e.to_string()))?;

                // Delete from legacy string table (v1.x)
                let mut string_table = tx
                    .open_table(crate::storage::schema::TABLE_NODE_PROPS)
                    .map_err(|e| Error::Other(e.to_string()))?;
                string_table
                    .remove(node_id)
                    .map_err(|e| Error::Other(e.to_string()))?;
            }

            #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
            fts_index::bump_committed_writes_in_txn(&tx, 1)?;

            tx.commit().map_err(|e| Error::Other(e.to_string()))?;
            self.store.after_write_commit();
        }

        #[cfg(target_arch = "wasm32")]
        self.store.delete_node_properties(node_id)?;

        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        if let Some(index) = self.vector_index.as_mut() {
            let _ = index.remove(node_id);
        }

        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        if let Some(index) = self.fts_index.as_mut() {
            let _ = index.delete_node(node_id);
        }

        Ok(())
    }

    fn node_has_relationships(&self, node_id: u64) -> bool {
        // Get type predicate ID
        let type_id = match self.resolve_id("type") {
            Ok(Some(id)) => id,
            _ => return false,
        };

        // Check if node is subject of any non-type triple
        let criteria_as_subject = QueryCriteria {
            subject_id: Some(node_id),
            predicate_id: None,
            object_id: None,
        };

        for triple in self.query(criteria_as_subject) {
            if triple.predicate_id != type_id {
                return true; // Found a relationship
            }
        }

        // Check if node is object of any triple (all relationships)
        let criteria_as_object = QueryCriteria {
            subject_id: None,
            predicate_id: None,
            object_id: Some(node_id),
        };

        self.query(criteria_as_object).next().is_some()
    }

    fn delete_all_relationships(&mut self, node_id: u64) -> Result<()> {
        // Get type predicate ID to exclude metadata triples
        let type_id = self
            .resolve_id("type")?
            .ok_or_else(|| Error::Other("Type predicate not found".to_string()))?;

        // Delete all triples where node is subject (except type metadata)
        let criteria_as_subject = QueryCriteria {
            subject_id: Some(node_id),
            predicate_id: None,
            object_id: None,
        };

        let triples_to_delete: Vec<Triple> = self
            .query(criteria_as_subject)
            .filter(|t| t.predicate_id != type_id)
            .collect();

        for triple in triples_to_delete {
            self.store.delete(&triple)?;
        }

        // Delete all triples where node is object
        let criteria_as_object = QueryCriteria {
            subject_id: None,
            predicate_id: None,
            object_id: Some(node_id),
        };

        let triples_to_delete: Vec<Triple> = self.query(criteria_as_object).collect();

        for triple in triples_to_delete {
            self.store.delete(&triple)?;
        }

        Ok(())
    }

    pub fn query(&self, criteria: QueryCriteria) -> crate::storage::HexastoreIter {
        self.store.query(
            criteria.subject_id,
            criteria.predicate_id,
            criteria.object_id,
        )
    }

    /// Get a reference to the underlying Hexastore for algorithm operations
    pub fn get_store(&self) -> &dyn crate::storage::Hexastore {
        self.store.as_ref()
    }

    pub fn open_cursor(&mut self, criteria: QueryCriteria) -> Result<u64> {
        let iter = self.query(criteria);
        let cursor_id = self.next_cursor_id;
        self.next_cursor_id = self.next_cursor_id.wrapping_add(1).max(1);
        self.cursors.insert(cursor_id, QueryCursor::new(iter));
        Ok(cursor_id)
    }

    pub fn cursor_next(
        &mut self,
        cursor_id: u64,
        batch_size: usize,
    ) -> Result<(Vec<Triple>, bool)> {
        let cursor = self
            .cursors
            .get_mut(&cursor_id)
            .ok_or(Error::InvalidCursor(cursor_id))?;
        let (batch, done) = cursor.next_batch(batch_size.max(1));
        if done {
            self.cursors.remove(&cursor_id);
        }
        Ok((batch, done))
    }

    pub fn close_cursor(&mut self, cursor_id: u64) -> Result<()> {
        self.cursors
            .remove(&cursor_id)
            .ok_or(Error::InvalidCursor(cursor_id))?;
        Ok(())
    }

    fn reset_cursors(&mut self) {
        self.cursors.clear();
        self.next_cursor_id = 1;
    }

    #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
    pub fn temporal_store(&self) -> &TemporalStore {
        &self.temporal
    }

    #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
    pub fn temporal_store_mut(&mut self) -> &mut TemporalStore {
        &mut self.temporal
    }

    #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
    pub fn timeline_query(&self, query: TimelineQuery) -> Vec<StoredFact> {
        self.temporal.query_timeline(&query).unwrap_or_default()
    }

    #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
    pub fn timeline_trace(&self, fact_id: u64) -> Vec<StoredEpisode> {
        self.temporal.trace_back(fact_id).unwrap_or_default()
    }

    // ========================================================================
    // Enhanced Transaction API
    // ========================================================================

    /// Begin a new write transaction
    ///
    /// Only one write transaction can be active at a time.
    /// All data operations will be buffered until commit().
    #[cfg(not(target_arch = "wasm32"))]
    pub fn begin_transaction(&mut self) -> Result<()> {
        if self.active_write.is_some() {
            return Err(Error::Other("transaction already open".to_string()));
        }
        let tx = self
            .redb
            .begin_write()
            .map_err(|e| Error::Other(e.to_string()))?;
        self.active_write = Some(tx);
        Ok(())
    }

    /// Commit the active transaction, making all changes durable
    #[cfg(not(target_arch = "wasm32"))]
    pub fn commit_transaction(&mut self) -> Result<()> {
        let tx = self
            .active_write
            .take()
            .ok_or_else(|| Error::Other("no active transaction".to_string()))?;
        tx.commit().map_err(|e| Error::Other(e.to_string()))?;
        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        {
            let staged = std::mem::take(&mut self.fts_write_log);
            if let Some(index) = self.fts_index.as_mut() {
                for (node_id, value) in staged {
                    if let Ok(props) = crate::storage::property::deserialize_properties(&value) {
                        let _ = index.upsert_from_props(node_id, &props);
                    }
                }
            }
        }
        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        self.vector_undo_log.clear();
        self.store.after_write_commit();
        Ok(())
    }

    /// Abort the active transaction, discarding all changes
    #[cfg(not(target_arch = "wasm32"))]
    pub fn abort_transaction(&mut self) -> Result<()> {
        #[cfg(all(feature = "fts", not(target_arch = "wasm32")))]
        self.fts_write_log.clear();
        #[cfg(all(feature = "vector", not(target_arch = "wasm32")))]
        self.rollback_vector_index();
        self.active_write = None;
        Ok(())
    }

    /// Check if a transaction is currently active
    #[cfg(not(target_arch = "wasm32"))]
    pub fn is_transaction_active(&self) -> bool {
        self.active_write.is_some()
    }

    /// Execute a closure within a transaction, automatically committing on success
    /// or aborting on error. This provides RAII-style transaction management.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_transaction<F, R>(&mut self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if self.is_transaction_active() {
            return Err(Error::Other("transaction already active".to_string()));
        }

        self.begin_transaction()?;

        match f(self) {
            Ok(result) => {
                self.commit_transaction()?;
                Ok(result)
            }
            Err(error) => {
                // Best effort abort on error
                let _ = self.abort_transaction();
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn open_and_insert() {
        let tmp = tempdir().unwrap();
        let mut db = Database::open(Options::new(tmp.path())).unwrap();
        let triple = db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();
        assert_eq!(db.all_triples(), vec![triple]);
        assert_eq!(db.resolve_str(triple.subject_id).unwrap().unwrap(), "alice");

        let results: Vec<_> = db
            .query(QueryCriteria {
                subject_id: Some(triple.subject_id),
                predicate_id: None,
                object_id: None,
            })
            .collect();
        assert_eq!(results, vec![triple]);

        let cursor_id = db
            .open_cursor(QueryCriteria {
                subject_id: Some(triple.subject_id),
                predicate_id: None,
                object_id: None,
            })
            .unwrap();
        let (batch, done) = db.cursor_next(cursor_id, 10).unwrap();
        assert!(done);
        assert_eq!(batch, vec![triple]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn node_and_edge_properties_roundtrip() {
        let tmp = tempdir().unwrap();
        let mut db = Database::open(Options::new(tmp.path())).unwrap();
        let triple = db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();

        db.set_node_property(triple.subject_id, r#"{"name":"alice"}"#)
            .unwrap();
        db.set_edge_property(
            triple.subject_id,
            triple.predicate_id,
            triple.object_id,
            r#"{"since":2020}"#,
        )
        .unwrap();

        assert_eq!(
            db.get_node_property(triple.subject_id).unwrap().unwrap(),
            r#"{"name":"alice"}"#
        );
        assert_eq!(
            db.get_edge_property(triple.subject_id, triple.predicate_id, triple.object_id)
                .unwrap()
                .unwrap(),
            r#"{"since":2020}"#
        );
    }

    #[test]
    #[cfg(all(feature = "temporal", not(target_arch = "wasm32")))]
    fn timeline_query_via_database() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("timeline-db");
        let mut db = Database::open(Options::new(&path)).unwrap();

        {
            let store = db.temporal_store_mut();
            let alice = store
                .ensure_entity(
                    "agent",
                    "alice",
                    EnsureEntityOptions {
                        alias: Some("Alice".into()),
                        occurred_at: Some("2025-01-01T00:00:00Z".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            let bob = store
                .ensure_entity(
                    "agent",
                    "bob",
                    EnsureEntityOptions {
                        alias: Some("Bob".into()),
                        occurred_at: Some("2025-01-01T00:00:00Z".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            let episode = store
                .add_episode(EpisodeInput {
                    source_type: "conversation".into(),
                    payload: serde_json::json!({ "text": "hello" }),
                    occurred_at: "2025-01-01T00:00:00Z".into(),
                    trace_hash: None,
                })
                .unwrap();
            let fact = store
                .upsert_fact(FactWriteInput {
                    subject_entity_id: alice.entity_id,
                    predicate_key: "mentions".into(),
                    object_entity_id: Some(bob.entity_id),
                    object_value: None,
                    valid_from: Some("2025-01-01T00:00:00Z".into()),
                    valid_to: None,
                    confidence: None,
                    source_episode_id: episode.episode_id,
                })
                .unwrap();
            store
                .link_episode(
                    episode.episode_id,
                    EpisodeLinkOptions {
                        entity_id: Some(alice.entity_id),
                        fact_id: Some(fact.fact_id),
                        role: "author".into(),
                    },
                )
                .unwrap();
        }

        let alice_id = db.temporal_store().get_entities().unwrap()[0].entity_id;
        let timeline = db.timeline_query(TimelineQuery {
            entity_id: alice_id,
            predicate_key: Some("mentions".into()),
            role: Some(TimelineRole::Subject),
            ..Default::default()
        });
        assert_eq!(timeline.len(), 1);

        let episodes = db.timeline_trace(timeline[0].fact_id);
        assert_eq!(episodes.len(), 1);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_transaction_api() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("tx_test.nervus");
        let mut db = Database::open(Options::new(&path)).unwrap();

        // Test basic transaction operations
        assert!(!db.is_transaction_active());

        db.begin_transaction().unwrap();
        assert!(db.is_transaction_active());

        // Add data within transaction
        db.add_fact(Fact::new("alice", "knows", "bob")).unwrap();

        // Commit
        db.commit_transaction().unwrap();
        assert!(!db.is_transaction_active());

        // Verify data persisted
        let query_result = db.query(QueryCriteria::default()).count();
        assert!(query_result > 0);

        // Test abort transaction
        db.begin_transaction().unwrap();
        db.add_fact(Fact::new("bob", "knows", "charlie")).unwrap();
        db.abort_transaction().unwrap();

        // Data should not persist after abort
        let alice_knows_bob = db.query(QueryCriteria::default()).count();
        let should_be_same = db.query(QueryCriteria::default()).count();
        assert_eq!(alice_knows_bob, should_be_same);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_with_transaction_api() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("with_tx_test.nervus");
        let mut db = Database::open(Options::new(&path)).unwrap();

        // Test successful transaction
        let result = db.with_transaction(|db| {
            db.add_fact(Fact::new("alice", "knows", "bob"))?;
            db.add_fact(Fact::new("bob", "knows", "charlie"))?;
            Ok("success")
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert!(!db.is_transaction_active());

        // Verify data committed
        let triples_count = db.query(QueryCriteria::default()).count();
        assert!(triples_count >= 2);

        // Test failed transaction (should rollback)
        let original_count = db.query(QueryCriteria::default()).count();
        let result: Result<&str> = db.with_transaction(|db| {
            db.add_fact(Fact::new("dave", "knows", "eve"))?;
            // Simulate error
            Err(crate::Error::Other("simulated error".to_string()))
        });
        assert!(result.is_err());
        assert!(!db.is_transaction_active());

        // Verify data was rolled back
        let final_count = db.query(QueryCriteria::default()).count();
        assert_eq!(original_count, final_count);
    }
}
