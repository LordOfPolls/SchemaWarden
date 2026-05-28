mod diff;
mod fetcher;
mod schema;
mod std_display;
mod sql_normalise;

use anyhow::Context;
use clap::Parser;
use futures::stream::{self, StreamExt, TryStreamExt};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;
use tiberius::{AuthMethod, Client, Config, Query};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tracing::{Instrument, debug, error, info, info_span, warn};
use tracing_subscriber::EnvFilter;
use crate::std_display::print_version_summary;

const TENANT_LIST_MAX: usize = 80;

pub fn get_version_label(version_idx: usize) -> String {
    let letter = (b'A' + (version_idx % 26) as u8) as char;
    let cycle = version_idx / 26;

    if cycle == 0 {
        letter.to_string()
    } else {
        format!("{}{}", letter, cycle)
    }
}

#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Debug)]
pub struct ServerHost {
    hostname: String,
    port: Option<u16>,
}

impl FromStr for ServerHost {
    type Err = anyhow::Error;

    fn from_str(host: &str) -> Result<Self, Self::Err> {
        match host.split_once(':') {
            Some((hostname, port_part)) => {
                let port = port_part
                    .parse()
                    .with_context(|| format!("Invalid port in server host: {host}"))?;
                Ok(Self {
                    hostname: hostname.to_string(),
                    port: Some(port),
                })
            }
            None => Ok(Self {
                hostname: host.to_string(),
                port: None,
            }),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Schema drift detection for multi-tenant MSSQL databases"
)]
pub struct Args {
    #[clap(
        short = 'H',
        long,
        value_delimiter = ',',
        default_value = "localhost",
        env = "SCHEMA_WARDEN_DB_HOST",
        help = "SQL Server host. Repeat for multiple hosts. Use host:port for non-default ports (e.g. myserver:1435)"
    )]
    db_host: Vec<ServerHost>,

    #[clap(
        long,
        short = 'u',
        env = "SCHEMA_WARDEN_DB_USER",
        help = "SQL Server login username"
    )]
    db_user: String,

    #[clap(
        long = "db-password",
        short = 'p',
        env = "SCHEMA_WARDEN_DB_PWD",
        alias = "db-pwd",
        hide_env_values = true,
        help = "SQL Server login password"
    )]
    db_pwd: String,

    #[clap(
        long,
        short,
        env = "SCHEMA_WARDEN_BASELINE_DB",
        help = "Name of the database to be treated as the source of truth"
    )]
    baseline_db: String,

    #[clap(
        long,
        env = "SCHEMA_WARDEN_BASELINE_HOST",
        help = "Baseline database host, defaults first db_host"
    )]
    baseline_host: Option<ServerHost>,

    #[clap(
        long,
        short,
        value_delimiter = ',',
        env = "SCHEMA_WARDEN_EXCLUDE_DATABASES",
        help = "Databases to exclude. Comma-separated or repeated flags: -e db1,db2 or -e db1 -e db2"
    )]
    exclude_databases: Vec<String>,

    #[clap(
        long,
        short,
        env = "SCHEMA_WARDEN_TRUST_CERT",
        help = "Trust the server's cert without verification"
    )]
    trust_cert: bool,

    #[clap(
        long,
        env = "SCHEMA_WARDEN_OBJECT",
        help = "Limit diff to a specific object. Format: [schema.]name — defaults to dbo if schema is omitted (e.g. --object MyTable or --object dbo.MyTable)"
    )]
    object: Option<String>,

    #[clap(
        long,
        short = 'c',
        default_value_t = 4,
        env = "SCHEMA_WARDEN_CONCURRENCY",
        help = "Maximum number of tenant databases to scan in parallel"
    )]
    concurrency: usize,

    #[clap(
        long,
        value_enum,
        default_value_t,
        help = "Output format (text or json)"
    )]
    format: OutputFormat,

    #[clap(
        long,
        short = 'o',
        env = "SCHEMA_WARDEN_OUTPUT",
        help = "Write output to this file instead of stdout"
    )]
    output: Option<PathBuf>,

    #[clap(
        long,
        env = "SCHEMA_WARDEN_DIFF_DIR",
        requires = "object",
        help = "Write a unified-diff file per drifted tenant. Requires --object pointing at a module-type object (view/procedure/function/trigger)"
    )]
    diff_dir: Option<PathBuf>,

    #[clap(
        long,
        short = 'v',
        env = "SCHEMA_WARDEN_VERBOSE",
        help = "Enable diagnostic logging. RUST_LOG overrides the default level when set."
    )]
    verbose: bool,
}

fn init_tracing(verbose: bool) {
    if !verbose {
        return;
    }
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("schema_warden=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

#[derive(Serialize)]
struct TenantReport {
    host: String,
    database: String,
    is_clean: bool,
    drift: diff::SchemaDiff,
}

fn parse_object_filter(s: &str) -> (String, String) {
    match s.split_once('.') {
        Some((schema, name)) => (schema.to_owned(), name.to_owned()),
        None => ("dbo".to_owned(), s.to_owned()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    init_tracing(args.verbose);

    if args.format == OutputFormat::Json && args.output.is_none() {
        eprintln!("error: JSON output requires --output <file>. Omit --format or use --format text for console output.");
        std::process::exit(2);
    }

    let started = Instant::now();
    let filter = args.object.as_deref().map(parse_object_filter);
    let filter_ref = filter.as_ref().map(|(s, n)| (s.as_str(), n.as_str()));

    let baseline_host = args
        .baseline_host
        .clone()
        .unwrap_or_else(|| args.db_host[0].clone());

    info!(
        hosts = args.db_host.len(),
        baseline_host = %baseline_host.hostname,
        baseline_db = %args.baseline_db,
        concurrency = args.concurrency,
        filter = ?filter,
        output = ?args.output,
        diff_dir = ?args.diff_dir,
        "starting schema-warden"
    );

    let baseline_started = Instant::now();
    let mut baseline_client = connect(&baseline_host, &args.baseline_db, &args).await?;
    let baseline =
        fetcher::fetch_schema(&mut baseline_client, &args.baseline_db, filter_ref).await?;
    info!(
        tables = baseline.tables.len(),
        views = baseline.views.len(),
        procedures = baseline.procedures.len(),
        functions = baseline.functions.len(),
        triggers = baseline.triggers.len(),
        elapsed_ms = baseline_started.elapsed().as_millis() as u64,
        "fetched baseline schema"
    );

    let tenants_by_host = stream::iter(args.db_host.iter().cloned())
        .then(|host| {
            let args = &args;
            async move {
                let discovered = fetch_tenants(&host, args).await?;
                let total = discovered.len();
                let tenants: Vec<_> = discovered
                    .into_iter()
                    .filter(|db| db != &args.baseline_db && !args.exclude_databases.contains(db))
                    .map(|db| (host.clone(), db))
                    .collect();
                info!(
                    host = %host.hostname,
                    discovered = total,
                    scanning = tenants.len(),
                    excluded = total - tenants.len(),
                    "discovered tenants"
                );
                Ok::<Vec<(ServerHost, String)>, anyhow::Error>(tenants)
            }
        })
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let concurrency = args.concurrency.max(1);
    let total_tenants = tenants_by_host.len();
    info!(total = total_tenants, concurrency, "scanning tenants");

    let mut reports: Vec<TenantReport> = stream::iter(tenants_by_host)
        .map(|(host, db)| {
            let baseline = &baseline;
            let args = &args;
            let span = info_span!("tenant", host = %host.hostname, db = %db);
            async move {
                let tenant_started = Instant::now();
                let mut client = connect(&host, &db, args).await.map_err(|e| {
                    error!(error = %e, "tenant connect failed");
                    e
                })?;
                let tenant = fetcher::fetch_schema(&mut client, &db, filter_ref)
                    .await
                    .map_err(|e| {
                        error!(error = %e, "tenant schema fetch failed");
                        e
                    })?;
                if tenant.tables.is_empty() && filter_ref.is_none() {
                    warn!(
                        "tenant returned zero tables; may indicate missing permissions"
                    );
                }
                let drift = diff::diff(baseline, &tenant);
                let is_clean = drift.is_clean();
                let module_changes = drift.views.len()
                    + drift.procedures.len()
                    + drift.functions.len()
                    + drift.triggers.len();
                info!(
                    clean = is_clean,
                    table_changes = drift.tables.len(),
                    module_changes,
                    elapsed_ms = tenant_started.elapsed().as_millis() as u64,
                    "tenant scan complete"
                );
                Ok::<TenantReport, anyhow::Error>(TenantReport {
                    host: host.hostname.clone(),
                    database: db.clone(),
                    is_clean,
                    drift,
                })
            }
            .instrument(span)
        })
        .buffer_unordered(concurrency)
        .try_collect()
        .await?;

    reports.sort_by(|a, b| a.host.cmp(&b.host).then(a.database.cmp(&b.database)));

    if let Some(diff_dir) = &args.diff_dir {
        let (filter_schema, filter_name) = filter.as_ref().unwrap();
        let object_key = format!("{}.{}", filter_schema, filter_name);

        if baseline.tables.contains_key(&object_key) {
            anyhow::bail!(
                "--diff-dir only supports module-type objects (view/procedure/function/trigger); '{}' is a table",
                object_key
            );
        }

        std::fs::create_dir_all(diff_dir)
            .with_context(|| format!("Failed to create diff directory: {}", diff_dir.display()))?;

        let mut files_written = 0usize;

        let sanitize = |s: &str| -> String {
            s.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        };
        
        let ambiguous_dbs = std_display::compute_ambiguous_dbs(&reports);
        let mut fp_groups: BTreeMap<String, Vec<(&TenantReport, &diff::ModuleChange)>> =
            BTreeMap::new();
        for report in &reports {
            let mc = report
                .drift
                .views
                .iter()
                .chain(report.drift.procedures.iter())
                .chain(report.drift.functions.iter())
                .chain(report.drift.triggers.iter())
                .next();
            let Some(mc) = mc else { continue };
            fp_groups
                .entry(mc.kind.fingerprint())
                .or_default()
                .push((report, mc));
        }

        let sorted_groups = std_display::order_drift_groups(fp_groups);

        for (version_idx, (_fp, group)) in sorted_groups.iter().enumerate() {
            let version_letter = get_version_label(version_idx + 1);
            let (_report, mc) = group[0];

            let (bl_text, tgt_text) = match &mc.kind {
                diff::ModuleChangeKind::DefinitionChanged { baseline, target } => {
                    (Some(baseline.as_str()), Some(target.as_str()))
                }
                diff::ModuleChangeKind::Added { definition } => (None, Some(definition.as_str())),
                diff::ModuleChangeKind::Removed { definition } => (Some(definition.as_str()), None),
            };

            let tenant_ids: Vec<String> = group
                .iter()
                .map(|(r, _)| format!("{}:{}", r.host, r.database))
                .collect();
            let tenant_list = std_display::truncate_tenant_list(&tenant_ids, &ambiguous_dbs);

            let header_baseline = format!(
                "baseline: {}/{} ({})",
                baseline_host.hostname, args.baseline_db, object_key
            );
            let header_target = format!(
                "Version {version_letter}: {tenant_list} ({})",
                object_key
            );

            let Some(patch) =
                diff::render_module_patch(bl_text, tgt_text, &header_baseline, &header_target)
            else {
                continue;
            };

            let filename = format!(
                "Version_{}__{}.diff",
                version_letter,
                sanitize(&object_key),
            );
            let path = diff_dir.join(&filename);
            std::fs::write(&path, &patch)
                .with_context(|| format!("Failed to write diff file: {}", path.display()))?;
            debug!(path = %path.display(), "wrote diff file");
            files_written += 1;
        }

        if files_written == 0 {
            warn!(
                object = %object_key,
                "--diff-dir produced no files; no module drift found for the filtered object"
            );
        } else {
            info!(files = files_written, dir = %diff_dir.display(), "wrote diff files");
        }
    }

    let mut out: Box<dyn Write> = match &args.output {
        Some(path) => Box::new(BufWriter::new(File::create(path).with_context(|| {
            format!("Failed to create output file: {}", path.display())
        })?)),
        None => Box::new(io::stdout().lock()),
    };

    match args.format {
        OutputFormat::Text => {
            print_version_summary(&reports, &mut out)?;
        }
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, &reports)?;
            writeln!(out)?;
        }
    }

    let drifted = reports.iter().filter(|r| !r.is_clean).count();
    let clean = reports.len() - drifted;
    let exit_code = if drifted > 0 { 1 } else { 0 };
    info!(
        scanned = reports.len(),
        clean,
        drifted,
        elapsed_s = started.elapsed().as_secs_f64(),
        exit = exit_code,
        "scan complete"
    );
    std::process::exit(exit_code);
}

pub async fn connect(
    host: &ServerHost,
    db_name: &str,
    args: &Args,
) -> anyhow::Result<Client<Compat<TcpStream>>> {
    let port = host.port.unwrap_or(1433);
    debug!(host = %host.hostname, port, db = %db_name, "connecting");
    let started = Instant::now();

    let mut config = Config::new();

    config.host(host.hostname.clone());
    config.port(port);
    config.readonly(true);
    config.authentication(AuthMethod::sql_server(
        args.db_user.clone(),
        args.db_pwd.clone(),
    ));

    if args.trust_cert {
        config.trust_cert();
    }
    config.database(db_name);

    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;

    let client = Client::connect(config, tcp.compat_write())
        .await
        .context("Failed to connect to database")?;

    debug!(
        host = %host.hostname,
        port,
        db = %db_name,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "connected"
    );
    Ok(client)
}

pub async fn fetch_tenants(host: &ServerHost, args: &Args) -> anyhow::Result<Vec<String>> {
    let mut client = connect(host, "master", args).await?;

    let sql = "
        SELECT name
        FROM sys.databases
        WHERE name NOT IN (
                'master'
                ,'model'
                ,'msdb'
                ,'tempdb'
                )
            AND STATE = 0
    ";

    let rows = Query::new(sql)
        .query(&mut client)
        .await?
        .into_first_result()
        .await;

    let mut tenants = Vec::new();
    for row in rows? {
        let name = row.get::<&str, _>(0).unwrap_or("").to_owned();
        tenants.push(name);
    }

    Ok(tenants)
}
