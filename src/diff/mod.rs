mod display;
mod types;

pub use types::*;

use indexmap::IndexMap;

use crate::schema::*;

pub fn diff(baseline: &DatabaseSchema, target: &DatabaseSchema) -> SchemaDiff {
    SchemaDiff {
        baseline_db: baseline.db_name.clone(),
        target_db: target.db_name.clone(),
        tables: diff_tables(&baseline.tables, &target.tables),
        views: diff_modules(&baseline.views, &target.views),
        procedures: diff_modules(&baseline.procedures, &target.procedures),
        functions: diff_modules(&baseline.functions, &target.functions),
        triggers: diff_modules(&baseline.triggers, &target.triggers),
    }
}

fn diff_tables(
    baseline: &IndexMap<String, TableDef>,
    target: &IndexMap<String, TableDef>,
) -> Vec<TableChange> {
    let mut changes = Vec::new();

    for key in baseline.keys() {
        if !target.contains_key(key) {
            changes.push(TableChange {
                key: key.clone(),
                kind: TableChangeKind::Removed {},
            });
        }
    }
    for key in target.keys() {
        if !baseline.contains_key(key) {
            changes.push(TableChange {
                key: key.clone(),
                kind: TableChangeKind::Added {},
            });
        }
    }
    for (key, b) in baseline {
        if let Some(t) = target.get(key) {
            let body = diff_table_body(b, t);
            if !body.is_empty() {
                changes.push(TableChange {
                    key: key.clone(),
                    kind: TableChangeKind::Modified(body),
                });
            }
        }
    }

    changes.sort_by(|a, b| a.key.cmp(&b.key));
    changes
}

fn diff_table_body(baseline: &TableDef, target: &TableDef) -> TableBodyDiff {
    TableBodyDiff {
        columns: diff_columns(&baseline.columns, &target.columns),
        indexes: diff_indexes(&baseline.indexes, &target.indexes),
        foreign_keys: diff_foreign_keys(&baseline.foreign_keys, &target.foreign_keys),
        check_constraints: diff_check_constraints(
            &baseline.check_constraints,
            &target.check_constraints,
        ),
    }
}

fn diff_columns(baseline: &[ColumnDef], target: &[ColumnDef]) -> Vec<ColumnChange> {
    let b: IndexMap<&str, &ColumnDef> = baseline.iter().map(|c| (c.name.as_str(), c)).collect();
    let t: IndexMap<&str, &ColumnDef> = target.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut changes = Vec::new();

    for name in b.keys() {
        if !t.contains_key(name) {
            changes.push(ColumnChange {
                name: name.to_string(),
                kind: ColumnChangeKind::Removed {},
            });
        }
    }
    for name in t.keys() {
        if !b.contains_key(name) {
            changes.push(ColumnChange {
                name: name.to_string(),
                kind: ColumnChangeKind::Added {},
            });
        }
    }
    for (name, bc) in &b {
        if let Some(tc) = t.get(name) {
            let fields = diff_column_fields(bc, tc);
            if !fields.is_empty() {
                changes.push(ColumnChange {
                    name: name.to_string(),
                    kind: ColumnChangeKind::Modified { fields },
                });
            }
        }
    }

    changes
}

fn diff_column_fields(b: &ColumnDef, t: &ColumnDef) -> Vec<ColumnField> {
    let mut f = Vec::new();
    if b.data_type != t.data_type {
        f.push(ColumnField::DataType {
            baseline: b.data_type.clone(),
            target: t.data_type.clone(),
        });
    }
    if b.max_length != t.max_length {
        f.push(ColumnField::MaxLength {
            baseline: b.max_length,
            target: t.max_length,
        });
    }
    if b.precision != t.precision {
        f.push(ColumnField::Precision {
            baseline: b.precision,
            target: t.precision,
        });
    }
    if b.scale != t.scale {
        f.push(ColumnField::Scale {
            baseline: b.scale,
            target: t.scale,
        });
    }
    if b.is_nullable != t.is_nullable {
        f.push(ColumnField::Nullable {
            baseline: b.is_nullable,
            target: t.is_nullable,
        });
    }
    if b.default_value != t.default_value {
        f.push(ColumnField::DefaultValue {
            baseline: b.default_value.clone(),
            target: t.default_value.clone(),
        });
    }
    if b.is_identity != t.is_identity {
        f.push(ColumnField::Identity {
            baseline: b.is_identity,
            target: t.is_identity,
        });
    }
    f
}

fn diff_indexes(baseline: &[IndexDef], target: &[IndexDef]) -> Vec<IndexChange> {
    let b: IndexMap<&str, &IndexDef> = baseline.iter().map(|i| (i.name.as_str(), i)).collect();
    let t: IndexMap<&str, &IndexDef> = target.iter().map(|i| (i.name.as_str(), i)).collect();
    let mut changes = Vec::new();

    for name in b.keys() {
        if !t.contains_key(name) {
            changes.push(IndexChange {
                name: name.to_string(),
                kind: IndexChangeKind::Removed {},
            });
        }
    }
    for name in t.keys() {
        if !b.contains_key(name) {
            changes.push(IndexChange {
                name: name.to_string(),
                kind: IndexChangeKind::Added {},
            });
        }
    }
    for (name, bi) in &b {
        if let Some(ti) = t.get(name) {
            let fields = diff_index_fields(bi, ti);
            if !fields.is_empty() {
                changes.push(IndexChange {
                    name: name.to_string(),
                    kind: IndexChangeKind::Modified { fields },
                });
            }
        }
    }

    changes
}

fn diff_index_fields(b: &IndexDef, t: &IndexDef) -> Vec<IndexField> {
    let mut f = Vec::new();
    if b.is_unique != t.is_unique {
        f.push(IndexField::Unique {
            baseline: b.is_unique,
            target: t.is_unique,
        });
    }
    if b.is_clustered != t.is_clustered {
        f.push(IndexField::Clustered {
            baseline: b.is_clustered,
            target: t.is_clustered,
        });
    }
    let cols_match = b.columns.len() == t.columns.len()
        && b.columns.iter().zip(&t.columns).all(|(bc, tc)| {
            bc.name == tc.name
                && bc.is_descending == tc.is_descending
                && bc.is_included == tc.is_included
        });
    if !cols_match {
        f.push(IndexField::Columns {
            baseline: b.columns.clone(),
            target: t.columns.clone(),
        });
    }
    f
}

fn diff_foreign_keys(baseline: &[ForeignKeyDef], target: &[ForeignKeyDef]) -> Vec<FkChange> {
    let b: IndexMap<&str, &ForeignKeyDef> =
        baseline.iter().map(|fk| (fk.name.as_str(), fk)).collect();
    let t: IndexMap<&str, &ForeignKeyDef> =
        target.iter().map(|fk| (fk.name.as_str(), fk)).collect();
    let mut changes = Vec::new();

    for name in b.keys() {
        if !t.contains_key(name) {
            changes.push(FkChange {
                name: name.to_string(),
                kind: FkChangeKind::Removed {},
            });
        }
    }
    for name in t.keys() {
        if !b.contains_key(name) {
            changes.push(FkChange {
                name: name.to_string(),
                kind: FkChangeKind::Added {},
            });
        }
    }
    for (name, bf) in &b {
        if let Some(tf) = t.get(name) {
            let fields = diff_fk_fields(bf, tf);
            if !fields.is_empty() {
                changes.push(FkChange {
                    name: name.to_string(),
                    kind: FkChangeKind::Modified { fields },
                });
            }
        }
    }

    changes
}

fn diff_fk_fields(b: &ForeignKeyDef, t: &ForeignKeyDef) -> Vec<FkField> {
    let mut f = Vec::new();
    if b.columns != t.columns {
        f.push(FkField::Columns {
            baseline: b.columns.clone(),
            target: t.columns.clone(),
        });
    }
    let b_ref = format!("{}.{}", b.ref_schema, b.ref_table);
    let t_ref = format!("{}.{}", t.ref_schema, t.ref_table);
    if b_ref != t_ref {
        f.push(FkField::RefTable {
            baseline: b_ref,
            target: t_ref,
        });
    }
    if b.ref_columns != t.ref_columns {
        f.push(FkField::RefColumns {
            baseline: b.ref_columns.clone(),
            target: t.ref_columns.clone(),
        });
    }
    if b.on_delete != t.on_delete {
        f.push(FkField::OnDelete {
            baseline: b.on_delete.clone(),
            target: t.on_delete.clone(),
        });
    }
    if b.on_update != t.on_update {
        f.push(FkField::OnUpdate {
            baseline: b.on_update.clone(),
            target: t.on_update.clone(),
        });
    }
    f
}

fn diff_check_constraints(
    baseline: &[CheckConstraintDef],
    target: &[CheckConstraintDef],
) -> Vec<ConstraintChange> {
    let b: IndexMap<&str, &CheckConstraintDef> =
        baseline.iter().map(|c| (c.name.as_str(), c)).collect();
    let t: IndexMap<&str, &CheckConstraintDef> =
        target.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut changes = Vec::new();

    for name in b.keys() {
        if !t.contains_key(name) {
            changes.push(ConstraintChange {
                name: name.to_string(),
                kind: ConstraintChangeKind::Removed {},
            });
        }
    }
    for name in t.keys() {
        if !b.contains_key(name) {
            changes.push(ConstraintChange {
                name: name.to_string(),
                kind: ConstraintChangeKind::Added {},
            });
        }
    }
    for (name, bc) in &b {
        if let Some(tc) = t.get(name)
            && bc.definition != tc.definition
        {
            changes.push(ConstraintChange {
                name: name.to_string(),
                kind: ConstraintChangeKind::DefinitionChanged {
                    baseline: bc.definition.clone(),
                    target: tc.definition.clone(),
                },
            });
        }
    }

    changes
}

fn diff_modules(
    baseline: &IndexMap<String, ModuleDef>,
    target: &IndexMap<String, ModuleDef>,
) -> Vec<ModuleChange> {
    let mut changes = Vec::new();

    for key in baseline.keys() {
        if !target.contains_key(key) {
            changes.push(ModuleChange {
                key: key.clone(),
                kind: ModuleChangeKind::Removed {},
            });
        }
    }
    for key in target.keys() {
        if !baseline.contains_key(key) {
            changes.push(ModuleChange {
                key: key.clone(),
                kind: ModuleChangeKind::Added {},
            });
        }
    }
    for (key, b) in baseline {
        if let Some(t) = target.get(key)
            && b.definition != t.definition
        {
            changes.push(ModuleChange {
                key: key.clone(),
                kind: ModuleChangeKind::DefinitionChanged {
                    baseline: b.definition.clone(),
                    target: t.definition.clone(),
                },
            });
        }
    }
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    changes
}
