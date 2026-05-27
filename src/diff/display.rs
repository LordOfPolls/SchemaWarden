use std::fmt;

use super::types::*;

impl fmt::Display for SchemaDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "=== Schema Drift: {} → {} ===",
            self.baseline_db, self.target_db
        )?;
        fmt_table_section(f, &self.tables)?;
        fmt_module_section(f, "VIEWS", &self.views)?;
        fmt_module_section(f, "PROCEDURES", &self.procedures)?;
        fmt_module_section(f, "FUNCTIONS", &self.functions)?;
        fmt_module_section(f, "TRIGGERS", &self.triggers)?;
        Ok(())
    }
}

fn fmt_table_section(f: &mut fmt::Formatter<'_>, changes: &[TableChange]) -> fmt::Result {
    writeln!(f, "\n[TABLES]")?;
    if changes.is_empty() {
        return writeln!(f, "  (no changes)");
    }
    for tc in changes {
        match &tc.kind {
            TableChangeKind::Added {} => writeln!(f, "  + {}", tc.key)?,
            TableChangeKind::Removed {} => writeln!(f, "  - {}", tc.key)?,
            TableChangeKind::Modified(body) => {
                writeln!(f, "  ~ {}", tc.key)?;
                fmt_body_diff(f, body)?;
            }
        }
    }
    Ok(())
}

fn fmt_body_diff(f: &mut fmt::Formatter<'_>, body: &TableBodyDiff) -> fmt::Result {
    if !body.columns.is_empty() {
        writeln!(f, "      columns:")?;
        for c in &body.columns {
            match &c.kind {
                ColumnChangeKind::Added {} => writeln!(f, "        + {}", c.name)?,
                ColumnChangeKind::Removed {} => writeln!(f, "        - {}", c.name)?,
                ColumnChangeKind::Modified { fields } => {
                    writeln!(f, "        ~ {}", c.name)?;
                    for field in fields {
                        writeln!(f, "            {field}")?;
                    }
                }
            }
        }
    }
    if !body.indexes.is_empty() {
        writeln!(f, "      indexes:")?;
        for i in &body.indexes {
            match &i.kind {
                IndexChangeKind::Added {} => writeln!(f, "        + {}", i.name)?,
                IndexChangeKind::Removed {} => writeln!(f, "        - {}", i.name)?,
                IndexChangeKind::Modified { fields } => {
                    writeln!(f, "        ~ {}", i.name)?;
                    for field in fields {
                        match field {
                            IndexField::Unique { baseline, target } => {
                                writeln!(f, "            unique: {baseline} → {target}")?
                            }
                            IndexField::Clustered { baseline, target } => {
                                writeln!(f, "            clustered: {baseline} → {target}")?
                            }
                            IndexField::Columns { baseline, target } => {
                                let fmt_cols = |cols: &[_]| -> Vec<String> {
                                    use crate::schema::IndexColumnRef;
                                    cols.iter()
                                        .map(|c: &IndexColumnRef| {
                                            format!(
                                                "{}{}",
                                                c.name,
                                                if c.is_descending { " DESC" } else { "" }
                                            )
                                        })
                                        .collect()
                                };
                                writeln!(
                                    f,
                                    "            columns: [{}] → [{}]",
                                    fmt_cols(baseline).join(", "),
                                    fmt_cols(target).join(", ")
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }
    if !body.foreign_keys.is_empty() {
        writeln!(f, "      foreign_keys:")?;
        for fk in &body.foreign_keys {
            match &fk.kind {
                FkChangeKind::Added {} => writeln!(f, "        + {}", fk.name)?,
                FkChangeKind::Removed {} => writeln!(f, "        - {}", fk.name)?,
                FkChangeKind::Modified { fields } => {
                    writeln!(f, "        ~ {}", fk.name)?;
                    for field in fields {
                        match field {
                            FkField::Columns { baseline, target } => {
                                writeln!(f, "            columns: {:?} → {:?}", baseline, target)?
                            }
                            FkField::RefTable { baseline, target } => {
                                writeln!(f, "            ref_table: {baseline} → {target}")?
                            }
                            FkField::RefColumns { baseline, target } => writeln!(
                                f,
                                "            ref_columns: {:?} → {:?}",
                                baseline, target
                            )?,
                            FkField::OnDelete { baseline, target } => {
                                writeln!(f, "            on_delete: {baseline} → {target}")?
                            }
                            FkField::OnUpdate { baseline, target } => {
                                writeln!(f, "            on_update: {baseline} → {target}")?
                            }
                        }
                    }
                }
            }
        }
    }
    if !body.check_constraints.is_empty() {
        writeln!(f, "      check_constraints:")?;
        for cc in &body.check_constraints {
            match &cc.kind {
                ConstraintChangeKind::Added {} => writeln!(f, "        + {}", cc.name)?,
                ConstraintChangeKind::Removed {} => writeln!(f, "        - {}", cc.name)?,
                ConstraintChangeKind::DefinitionChanged { .. } => {
                    writeln!(f, "        ~ {}: definition changed", cc.name)?
                }
            }
        }
    }
    Ok(())
}

fn fmt_module_section(
    f: &mut fmt::Formatter<'_>,
    label: &str,
    changes: &[ModuleChange],
) -> fmt::Result {
    writeln!(f, "\n[{label}]")?;
    if changes.is_empty() {
        return writeln!(f, "  (no changes)");
    }
    for mc in changes {
        match &mc.kind {
            ModuleChangeKind::Added {} => writeln!(f, "  + {}", mc.key)?,
            ModuleChangeKind::Removed {} => writeln!(f, "  - {}", mc.key)?,
            ModuleChangeKind::DefinitionChanged { .. } => {
                writeln!(f, "  ~ {}: definition changed", mc.key)?
            }
        }
    }
    Ok(())
}
