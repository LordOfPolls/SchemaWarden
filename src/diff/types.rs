use std::fmt;

use crate::schema::IndexColumnRef;

#[derive(Debug, Clone)]
pub struct SchemaDiff {
    pub baseline_db: String,
    pub target_db: String,
    pub tables: Vec<TableChange>,
    pub views: Vec<ModuleChange>,
    pub procedures: Vec<ModuleChange>,
    pub functions: Vec<ModuleChange>,
    pub triggers: Vec<ModuleChange>,
}

impl SchemaDiff {
    pub fn is_clean(&self) -> bool {
        self.tables.is_empty()
            && self.views.is_empty()
            && self.procedures.is_empty()
            && self.functions.is_empty()
            && self.triggers.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct TableChange {
    pub key: String,
    pub kind: TableChangeKind,
}

#[derive(Debug, Clone)]
pub enum TableChangeKind {
    Added,
    Removed,
    Modified(TableBodyDiff),
}

#[derive(Debug, Clone, Default)]
pub struct TableBodyDiff {
    pub columns: Vec<ColumnChange>,
    pub indexes: Vec<IndexChange>,
    pub foreign_keys: Vec<FkChange>,
    pub check_constraints: Vec<ConstraintChange>,
}

impl TableBodyDiff {
    pub(super) fn is_empty(&self) -> bool {
        self.columns.is_empty()
            && self.indexes.is_empty()
            && self.foreign_keys.is_empty()
            && self.check_constraints.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ColumnChange {
    pub name: String,
    pub kind: ColumnChangeKind,
}

#[derive(Debug, Clone)]
pub enum ColumnChangeKind {
    Added,
    Removed,
    Modified(Vec<ColumnField>),
}

#[derive(Debug, Clone)]
pub enum ColumnField {
    DataType {
        baseline: String,
        target: String,
    },
    MaxLength {
        baseline: Option<i32>,
        target: Option<i32>,
    },
    Precision {
        baseline: Option<u8>,
        target: Option<u8>,
    },
    Scale {
        baseline: Option<i32>,
        target: Option<i32>,
    },
    Nullable {
        baseline: bool,
        target: bool,
    },
    DefaultValue {
        baseline: Option<String>,
        target: Option<String>,
    },
    Identity {
        baseline: bool,
        target: bool,
    },
}

impl fmt::Display for ColumnField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataType { baseline, target } => write!(f, "data_type: {baseline} → {target}"),
            Self::MaxLength { baseline, target } => {
                write!(f, "max_length: {baseline:?} → {target:?}")
            }
            Self::Precision { baseline, target } => {
                write!(f, "precision: {baseline:?} → {target:?}")
            }
            Self::Scale { baseline, target } => write!(f, "scale: {baseline:?} → {target:?}"),
            Self::Nullable { baseline, target } => write!(f, "nullable: {baseline} → {target}"),
            Self::DefaultValue { baseline, target } => {
                write!(f, "default: {baseline:?} → {target:?}")
            }
            Self::Identity { baseline, target } => write!(f, "identity: {baseline} → {target}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexChange {
    pub name: String,
    pub kind: IndexChangeKind,
}

#[derive(Debug, Clone)]
pub enum IndexChangeKind {
    Added,
    Removed,
    Modified(Vec<IndexField>),
}

#[derive(Debug, Clone)]
pub enum IndexField {
    Unique {
        baseline: bool,
        target: bool,
    },
    Clustered {
        baseline: bool,
        target: bool,
    },
    Columns {
        baseline: Vec<IndexColumnRef>,
        target: Vec<IndexColumnRef>,
    },
}

#[derive(Debug, Clone)]
pub struct FkChange {
    pub name: String,
    pub kind: FkChangeKind,
}

#[derive(Debug, Clone)]
pub enum FkChangeKind {
    Added,
    Removed,
    Modified(Vec<FkField>),
}

#[derive(Debug, Clone)]
pub enum FkField {
    Columns {
        baseline: Vec<String>,
        target: Vec<String>,
    },
    RefTable {
        baseline: String,
        target: String,
    },
    RefColumns {
        baseline: Vec<String>,
        target: Vec<String>,
    },
    OnDelete {
        baseline: String,
        target: String,
    },
    OnUpdate {
        baseline: String,
        target: String,
    },
}

#[derive(Debug, Clone)]
pub struct ConstraintChange {
    pub name: String,
    pub kind: ConstraintChangeKind,
}

#[derive(Debug, Clone)]
pub enum ConstraintChangeKind {
    Added,
    Removed,
    DefinitionChanged { baseline: String, target: String },
}

#[derive(Debug, Clone)]
pub struct ModuleChange {
    pub key: String,
    pub kind: ModuleChangeKind,
}

#[derive(Debug, Clone)]
pub enum ModuleChangeKind {
    Added,
    Removed,
    DefinitionChanged { baseline: String, target: String },
}
