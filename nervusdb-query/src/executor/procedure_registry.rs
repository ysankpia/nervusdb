use super::{
    EdgeKey, Error, GraphSnapshot, InternalNodeId, LabelId, RelTypeId, Result, Row, Value,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock, RwLock};

pub trait Procedure: Send + Sync {
    fn execute(&self, snapshot: &dyn ErasedSnapshot, args: Vec<Value>) -> Result<Vec<Row>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestProcedureType {
    Any,
    Integer,
    Float,
    Number,
    String,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestProcedureField {
    pub name: String,
    pub field_type: TestProcedureType,
    pub nullable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TestProcedureFixture {
    pub inputs: Vec<TestProcedureField>,
    pub outputs: Vec<TestProcedureField>,
    pub rows: Vec<BTreeMap<String, Value>>,
}

pub trait ErasedSnapshot {
    fn neighbors_erased(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_>;
    fn incoming_neighbors_erased(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_>;
    fn node_property_erased(
        &self,
        iid: InternalNodeId,
        key: &str,
    ) -> Option<nervusdb_api::PropertyValue>;
    fn resolve_label_name_erased(&self, id: LabelId) -> Option<String>;
    fn resolve_rel_type_name_erased(&self, id: RelTypeId) -> Option<String>;
    fn resolve_node_labels_erased(&self, iid: InternalNodeId) -> Option<Vec<LabelId>>;
    fn node_properties_erased(
        &self,
        iid: InternalNodeId,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_api::PropertyValue>>;
    fn edge_properties_erased(
        &self,
        key: EdgeKey,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_api::PropertyValue>>;
}

impl<S: GraphSnapshot> ErasedSnapshot for S {
    fn neighbors_erased(
        &self,
        src: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        Box::new(self.neighbors(src, rel))
    }

    fn incoming_neighbors_erased(
        &self,
        dst: InternalNodeId,
        rel: Option<RelTypeId>,
    ) -> Box<dyn Iterator<Item = EdgeKey> + '_> {
        Box::new(self.incoming_neighbors(dst, rel))
    }

    fn node_property_erased(
        &self,
        iid: InternalNodeId,
        key: &str,
    ) -> Option<nervusdb_api::PropertyValue> {
        self.node_property(iid, key)
    }

    fn resolve_label_name_erased(&self, id: LabelId) -> Option<String> {
        self.resolve_label_name(id)
    }

    fn resolve_rel_type_name_erased(&self, id: RelTypeId) -> Option<String> {
        self.resolve_rel_type_name(id)
    }

    fn resolve_node_labels_erased(&self, iid: InternalNodeId) -> Option<Vec<LabelId>> {
        self.resolve_node_labels(iid)
    }

    fn node_properties_erased(
        &self,
        iid: InternalNodeId,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_api::PropertyValue>> {
        self.node_properties(iid)
    }

    fn edge_properties_erased(
        &self,
        key: EdgeKey,
    ) -> Option<std::collections::BTreeMap<String, nervusdb_api::PropertyValue>> {
        self.edge_properties(key)
    }
}

pub struct ProcedureRegistry {
    handlers: HashMap<String, Arc<dyn Procedure>>,
}

impl Default for ProcedureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcedureRegistry {
    pub fn new() -> Self {
        let mut handlers: HashMap<String, Arc<dyn Procedure>> = HashMap::new();
        handlers.insert("db.info".to_string(), Arc::new(DbInfoProcedure));
        handlers.insert("math.add".to_string(), Arc::new(MathAddProcedure));
        handlers.insert(
            "test.doNothing".to_string(),
            Arc::new(TestFixtureProcedure {
                name: "test.doNothing".to_string(),
            }),
        );
        handlers.insert(
            "test.labels".to_string(),
            Arc::new(TestFixtureProcedure {
                name: "test.labels".to_string(),
            }),
        );
        handlers.insert(
            "test.my.proc".to_string(),
            Arc::new(TestFixtureProcedure {
                name: "test.my.proc".to_string(),
            }),
        );
        Self { handlers }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Procedure>> {
        self.handlers.get(name).cloned()
    }
}

pub static GLOBAL_PROCEDURE_REGISTRY: OnceLock<ProcedureRegistry> = OnceLock::new();

pub fn get_procedure_registry() -> &'static ProcedureRegistry {
    GLOBAL_PROCEDURE_REGISTRY.get_or_init(ProcedureRegistry::new)
}

static TEST_PROCEDURE_FIXTURES: OnceLock<RwLock<HashMap<String, TestProcedureFixture>>> =
    OnceLock::new();

fn get_test_procedure_fixture_map() -> &'static RwLock<HashMap<String, TestProcedureFixture>> {
    TEST_PROCEDURE_FIXTURES.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn clear_test_procedure_fixtures() {
    if let Ok(mut guard) = get_test_procedure_fixture_map().write() {
        guard.clear();
    }
}

pub fn register_test_procedure_fixture(name: impl Into<String>, fixture: TestProcedureFixture) {
    if let Ok(mut guard) = get_test_procedure_fixture_map().write() {
        guard.insert(name.into(), fixture);
    }
}

pub fn get_test_procedure_fixture(name: &str) -> Option<TestProcedureFixture> {
    get_test_procedure_fixture_map()
        .read()
        .ok()
        .and_then(|guard| guard.get(name).cloned())
}

fn assert_assignable(field: &TestProcedureField, value: &Value) -> Result<()> {
    if matches!(value, Value::Null) {
        return if field.nullable {
            Ok(())
        } else {
            Err(Error::Other(
                "syntax error: InvalidArgumentType".to_string(),
            ))
        };
    }

    let ok = match field.field_type {
        TestProcedureType::Any => true,
        TestProcedureType::Integer => matches!(value, Value::Int(_)),
        TestProcedureType::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        TestProcedureType::Number => matches!(value, Value::Float(_) | Value::Int(_)),
        TestProcedureType::String => matches!(value, Value::String(_)),
        TestProcedureType::Boolean => matches!(value, Value::Bool(_)),
    };

    if ok {
        Ok(())
    } else {
        Err(Error::Other(
            "syntax error: InvalidArgumentType".to_string(),
        ))
    }
}

fn values_match(field: &TestProcedureField, expected: &Value, actual: &Value) -> bool {
    if matches!(expected, Value::Null) || matches!(actual, Value::Null) {
        return expected == actual;
    }

    match field.field_type {
        TestProcedureType::Float | TestProcedureType::Number => {
            let left = match expected {
                Value::Int(i) => *i as f64,
                Value::Float(f) => *f,
                _ => return false,
            };
            let right = match actual {
                Value::Int(i) => *i as f64,
                Value::Float(f) => *f,
                _ => return false,
            };
            (left - right).abs() < 1e-9
        }
        _ => expected == actual,
    }
}

struct TestFixtureProcedure {
    name: String,
}

impl Procedure for TestFixtureProcedure {
    fn execute(&self, _snapshot: &dyn ErasedSnapshot, args: Vec<Value>) -> Result<Vec<Row>> {
        let Some(fixture) = get_test_procedure_fixture(&self.name) else {
            return Err(Error::Other("syntax error: ProcedureNotFound".to_string()));
        };

        if args.len() != fixture.inputs.len() {
            return Err(Error::Other(
                "syntax error: InvalidNumberOfArguments".to_string(),
            ));
        }

        for (field, value) in fixture.inputs.iter().zip(args.iter()) {
            assert_assignable(field, value)?;
        }

        if fixture.outputs.is_empty() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for row in &fixture.rows {
            let mut matched = true;
            for (idx, field) in fixture.inputs.iter().enumerate() {
                let expected = row.get(&field.name);
                let actual = args.get(idx);
                match (expected, actual) {
                    (Some(expected), Some(actual)) if values_match(field, expected, actual) => {}
                    _ => {
                        matched = false;
                        break;
                    }
                }
            }

            if !matched {
                continue;
            }

            let mut cols = Vec::with_capacity(fixture.outputs.len());
            for field in &fixture.outputs {
                cols.push((
                    field.name.clone(),
                    row.get(&field.name).cloned().unwrap_or(Value::Null),
                ));
            }
            out.push(Row::new(cols));
        }

        Ok(out)
    }
}

struct DbInfoProcedure;

impl Procedure for DbInfoProcedure {
    fn execute(&self, _snapshot: &dyn ErasedSnapshot, _args: Vec<Value>) -> Result<Vec<Row>> {
        Ok(vec![Row::new(vec![(
            "version".to_string(),
            Value::String("2.0.0".to_string()),
        )])])
    }
}

struct MathAddProcedure;

impl Procedure for MathAddProcedure {
    fn execute(&self, _snapshot: &dyn ErasedSnapshot, args: Vec<Value>) -> Result<Vec<Row>> {
        if args.len() != 2 {
            return Err(Error::Other("math.add requires 2 arguments".to_string()));
        }
        let a = match &args[0] {
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            _ => {
                return Err(Error::Other(
                    "math.add requires numeric arguments".to_string(),
                ));
            }
        };
        let b = match &args[1] {
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            _ => {
                return Err(Error::Other(
                    "math.add requires numeric arguments".to_string(),
                ));
            }
        };
        Ok(vec![Row::new(vec![(
            "result".to_string(),
            Value::Float(a + b),
        )])])
    }
}
