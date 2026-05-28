use crate::{TENANT_LIST_MAX, TenantReport, diff, get_version_label};
use comfy_table::{Cell, Table};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub fn compute_ambiguous_dbs(reports: &[TenantReport]) -> BTreeSet<String> {
    let mut db_name_hosts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for r in reports {
        db_name_hosts
            .entry(r.database.clone())
            .or_default()
            .insert(r.host.clone());
    }
    db_name_hosts
        .into_iter()
        .filter(|(_, hosts)| hosts.len() > 1)
        .map(|(db, _)| db)
        .collect()
}

pub fn order_drift_groups<T>(groups: BTreeMap<String, Vec<T>>) -> Vec<(String, Vec<T>)> {
    let mut ordered: Vec<(String, Vec<T>)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    ordered
}

pub fn tenant_display_name(id: &str, ambiguous_dbs: &BTreeSet<String>) -> String {
    if let Some((_, db)) = id.split_once(':')
        && !ambiguous_dbs.contains(db)
    {
        return db.to_string();
    }
    id.to_string()
}

pub fn truncate_tenant_list(tenant_ids: &[String], ambiguous_dbs: &BTreeSet<String>) -> String {
    let mut out = String::new();
    for (i, id) in tenant_ids.iter().enumerate() {
        let t = tenant_display_name(id, ambiguous_dbs);
        let sep = if i == 0 { "" } else { ", " };
        let candidate = format!("{sep}{t}");
        if out.len() + candidate.len() > TENANT_LIST_MAX {
            out.push_str(", ...");
            break;
        }
        out.push_str(&candidate);
    }
    out
}

fn make_row(
    label: String,
    tenants: &[String],
    ambiguous_dbs: &BTreeSet<String>,
    is_baseline: bool,
) -> (String, String, usize, bool) {
    (
        label,
        truncate_tenant_list(tenants, ambiguous_dbs),
        tenants.len(),
        is_baseline,
    )
}

fn version_label(idx: usize, is_baseline: bool) -> String {
    let letter = get_version_label(idx);
    if is_baseline {
        format!("Version {letter} (baseline)")
    } else {
        format!("Version {letter}")
    }
}

struct ObjectSection {
    type_label: &'static str,
    object_key: String,
    /// (version_label, tenant_list_display, count, matches_baseline)
    rows: Vec<(String, String, usize, bool)>,
}

fn build_module_sections(
    all_tenant_ids: &[String],
    ambiguous_dbs: &BTreeSet<String>,
    reports: &[TenantReport],
    type_label: &'static str,
    get_changes: impl Fn(&TenantReport) -> &[diff::ModuleChange],
) -> Vec<ObjectSection> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for r in reports {
        for mc in get_changes(r) {
            keys.insert(mc.key.clone());
        }
    }

    let mut sections = Vec::new();

    for key in &keys {
        let mut fingerprint_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for r in reports {
            let tenant_id = format!("{}:{}", r.host, r.database);
            let change = get_changes(r).iter().find(|mc| &mc.key == key);
            let fingerprint = match change {
                None => continue,
                Some(mc) => mc.kind.fingerprint(),
            };
            fingerprint_map
                .entry(fingerprint)
                .or_default()
                .push(tenant_id);
        }

        if fingerprint_map.is_empty() {
            continue;
        }

        let drifted_ids: BTreeSet<String> = fingerprint_map.values().flatten().cloned().collect();
        let baseline_tenants: Vec<String> = all_tenant_ids
            .iter()
            .filter(|id| !drifted_ids.contains(*id))
            .cloned()
            .collect();

        let non_baseline = order_drift_groups(fingerprint_map);

        let mut rows = Vec::new();
        rows.push(make_row(
            version_label(0, true),
            &baseline_tenants,
            ambiguous_dbs,
            true,
        ));
        for (i, (_fp, tenants)) in non_baseline.iter().enumerate() {
            rows.push(make_row(
                version_label(i + 1, false),
                tenants,
                ambiguous_dbs,
                false,
            ));
        }

        if non_baseline.is_empty() {
            continue;
        }

        sections.push(ObjectSection {
            type_label,
            object_key: key.clone(),
            rows,
        });
    }

    sections
}

fn build_table_sections(
    all_tenant_ids: &[String],
    ambiguous_dbs: &BTreeSet<String>,
    reports: &[TenantReport],
) -> Vec<ObjectSection> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for r in reports {
        for tc in &r.drift.tables {
            keys.insert(tc.key.clone());
        }
    }

    let mut sections = Vec::new();

    for key in &keys {
        let mut fingerprint_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for r in reports {
            let tenant_id = format!("{}:{}", r.host, r.database);
            let change = r.drift.tables.iter().find(|tc| &tc.key == key);
            let fingerprint = match change {
                None => continue,
                Some(tc) => match &tc.kind {
                    diff::TableChangeKind::Modified(body) => {
                        serde_json::to_string(body).unwrap_or_default()
                    }
                    diff::TableChangeKind::Removed {} => "__REMOVED__".to_string(),
                    diff::TableChangeKind::Added {} => "__ADDED__".to_string(),
                },
            };
            fingerprint_map
                .entry(fingerprint)
                .or_default()
                .push(tenant_id);
        }

        if fingerprint_map.is_empty() {
            continue;
        }

        let drifted_ids: BTreeSet<String> = fingerprint_map.values().flatten().cloned().collect();
        let baseline_tenants: Vec<String> = all_tenant_ids
            .iter()
            .filter(|id| !drifted_ids.contains(*id))
            .cloned()
            .collect();

        let non_baseline = order_drift_groups(fingerprint_map);

        let mut rows = Vec::new();
        rows.push(make_row(
            version_label(0, true),
            &baseline_tenants,
            ambiguous_dbs,
            true,
        ));
        for (i, (_fp, tenants)) in non_baseline.iter().enumerate() {
            rows.push(make_row(
                version_label(i + 1, false),
                tenants,
                ambiguous_dbs,
                false,
            ));
        }

        if non_baseline.is_empty() {
            continue;
        }

        sections.push(ObjectSection {
            type_label: "TABLE",
            object_key: key.clone(),
            rows,
        });
    }

    sections
}

pub fn print_version_summary(reports: &[TenantReport], out: &mut dyn Write) -> anyhow::Result<()> {
    let failed: Vec<&TenantReport> = reports.iter().filter(|r| r.error.is_some()).collect();
    let succeeded: Vec<TenantReport> = reports
        .iter()
        .filter(|r| r.error.is_none())
        .cloned()
        .collect();

    let ambiguous_dbs = compute_ambiguous_dbs(&succeeded);

    let all_tenant_ids: Vec<String> = succeeded
        .iter()
        .map(|r| format!("{}:{}", r.host, r.database))
        .collect();

    let mut sections: Vec<ObjectSection> = Vec::new();
    sections.extend(build_table_sections(
        &all_tenant_ids,
        &ambiguous_dbs,
        &succeeded,
    ));
    sections.extend(build_module_sections(
        &all_tenant_ids,
        &ambiguous_dbs,
        &succeeded,
        "VIEW",
        |r| &r.drift.views,
    ));
    sections.extend(build_module_sections(
        &all_tenant_ids,
        &ambiguous_dbs,
        &succeeded,
        "PROCEDURE",
        |r| &r.drift.procedures,
    ));
    sections.extend(build_module_sections(
        &all_tenant_ids,
        &ambiguous_dbs,
        &succeeded,
        "FUNCTION",
        |r| &r.drift.functions,
    ));
    sections.extend(build_module_sections(
        &all_tenant_ids,
        &ambiguous_dbs,
        &succeeded,
        "TRIGGER",
        |r| &r.drift.triggers,
    ));

    if sections.is_empty() && failed.is_empty() {
        writeln!(
            out,
            "All {} tenant(s) match baseline. No schema drift detected.",
            reports.len()
        )?;
        return Ok(());
    }

    if !sections.is_empty() {
        for (i, section) in sections.iter().enumerate() {
            if i > 0 {
                writeln!(out)?;
            }
            writeln!(out, "{} | {}", section.type_label, section.object_key)?;

            let mut table = Table::new();
            table.load_preset(comfy_table::presets::ASCII_NO_BORDERS);
            table.set_header(vec!["Version", "Tenants", "Total DBs", "Matches Baseline"]);

            for (label, tenant_str, count, is_baseline) in &section.rows {
                let db_word = if *count == 1 { "db" } else { "dbs" };
                table.add_row(vec![
                    Cell::new(label),
                    Cell::new(tenant_str),
                    Cell::new(format!("{count} {db_word}")),
                    Cell::new(if *is_baseline { "yes" } else { "no" }),
                ]);
            }

            writeln!(out, "{table}")?;
        }
    } else {
        writeln!(
            out,
            "All {} reachable tenant(s) match baseline. No schema drift detected.",
            succeeded.len()
        )?;
    }

    if !failed.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "SCAN ERRORS | {} tenant(s) could not be scanned",
            failed.len()
        )?;
        let mut table = Table::new();
        table.load_preset(comfy_table::presets::ASCII_NO_BORDERS);
        table.set_header(vec!["Host", "Database", "Error"]);
        for r in &failed {
            table.add_row(vec![
                Cell::new(&r.host),
                Cell::new(&r.database),
                Cell::new(r.error.as_deref().unwrap_or("")),
            ]);
        }
        writeln!(out, "{table}")?;
    }

    Ok(())
}
