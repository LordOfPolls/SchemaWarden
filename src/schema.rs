use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    pub db_name: String,
    pub tables: IndexMap<String, TableDef>,
    pub views: IndexMap<String, ModuleDef>,
    pub procedures: IndexMap<String, ModuleDef>,
    pub functions: IndexMap<String, ModuleDef>,
    pub triggers: IndexMap<String, ModuleDef>,
}

impl DatabaseSchema {
    pub fn new(db_name: impl Into<String>) -> Self {
        Self {
            db_name: db_name.into(),
            tables: IndexMap::new(),
            views: IndexMap::new(),
            procedures: IndexMap::new(),
            functions: IndexMap::new(),
            triggers: IndexMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDef {
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub check_constraints: Vec<CheckConstraintDef>,
}

impl TableDef {
    pub fn new(schema: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            name: name.into(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub ordinal: i32,
    pub data_type: String,
    pub max_length: Option<i32>,
    pub precision: Option<u8>,
    pub scale: Option<i32>,
    pub is_nullable: bool,
    pub default_value: Option<String>,
    pub is_identity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDef {
    pub name: String,
    pub is_unique: bool,
    pub is_primary_key: bool,
    pub is_clustered: bool,
    pub columns: Vec<IndexColumnRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexColumnRef {
    pub name: String,
    pub is_descending: bool,
    pub is_included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyDef {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_delete: String,
    pub on_update: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConstraintDef {
    pub name: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDef {
    pub schema: String,
    pub name: String,
    pub object_type: ObjectType,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObjectType {
    View,
    StoredProcedure,
    ScalarFunction,
    InlineTableValuedFunction,
    MultiStatementTableValuedFunction,
    Trigger,
}

impl ObjectType {
    pub fn from_type_desc(type_desc: &str) -> Option<Self> {
        match type_desc {
            "VIEW" => Some(Self::View),
            "SQL_STORED_PROCEDURE" => Some(Self::StoredProcedure),
            "SQL_SCALAR_FUNCTION" => Some(Self::ScalarFunction),
            "SQL_INLINE_TABLE_VALUED_FUNCTION" => Some(Self::InlineTableValuedFunction),
            "SQL_TABLE_VALUED_FUNCTION" => Some(Self::MultiStatementTableValuedFunction),
            "SQL_TRIGGER" => Some(Self::Trigger),
            _ => None,
        }
    }
}
