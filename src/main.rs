mod diff;
mod fetcher;
mod schema;

use anyhow::Context;
use clap::Parser;
use futures::stream::{self, StreamExt, TryStreamExt};
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::str::FromStr;
use tiberius::{AuthMethod, Client, Config, Query};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

#[derive(clap::ValueEnum, Clone, Debug, Default)]
enum OutputFormat {
    Text,
    #[default]
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
        let mut host = host.replace(':', ";");
        let mut port = 1433;

        if let Some((host_part, port_part)) = host.split_once(';') {
            port = port_part
                .parse()
                .with_context(|| format!("Invalid port in server host: {host}"))?;
            host = host_part.to_string();
        }

        Ok(Self {
            hostname: host,
            port: Some(port),
        })
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

    let filter = args.object.as_deref().map(parse_object_filter);
    let filter_ref = filter.as_ref().map(|(s, n)| (s.as_str(), n.as_str()));

    let baseline_host = args
        .baseline_host
        .clone()
        .unwrap_or_else(|| args.db_host[0].clone());

    let mut baseline_client = connect(&baseline_host, &args.baseline_db, &args).await?;
    let baseline =
        fetcher::fetch_schema(&mut baseline_client, &args.baseline_db, filter_ref).await?;

    let tenants_by_host = stream::iter(args.db_host.iter().cloned())
        .then(|host| {
            let args = &args;
            async move {
                let tenants = fetch_tenants(&host, args).await?;
                Ok::<Vec<(ServerHost, String)>, anyhow::Error>(
                    tenants
                        .into_iter()
                        .filter(|db| {
                            db != &args.baseline_db && !args.exclude_databases.contains(db)
                        })
                        .map(|db| (host.clone(), db))
                        .collect(),
                )
            }
        })
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let concurrency = args.concurrency.max(1);

    let mut reports: Vec<TenantReport> = stream::iter(tenants_by_host)
        .map(|(host, db)| {
            let baseline = &baseline;
            let args = &args;
            async move {
                let mut client = connect(&host, &db, args).await?;
                let tenant = fetcher::fetch_schema(&mut client, &db, filter_ref).await?;
                let drift = diff::diff(baseline, &tenant);
                let is_clean = drift.is_clean();
                Ok::<TenantReport, anyhow::Error>(TenantReport {
                    host: host.hostname.clone(),
                    database: db.clone(),
                    is_clean,
                    drift,
                })
            }
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

        for report in &reports {
            let module_change = report
                .drift
                .views
                .iter()
                .chain(report.drift.procedures.iter())
                .chain(report.drift.functions.iter())
                .chain(report.drift.triggers.iter())
                .next();

            let Some(mc) = module_change else { continue };

            let (bl_text, tgt_text) = match &mc.kind {
                diff::ModuleChangeKind::DefinitionChanged { baseline, target } => {
                    (Some(baseline.as_str()), Some(target.as_str()))
                }
                diff::ModuleChangeKind::Added { definition } => (None, Some(definition.as_str())),
                diff::ModuleChangeKind::Removed { definition } => (Some(definition.as_str()), None),
            };

            let header_baseline = format!(
                "baseline: {}/{} ({})",
                baseline_host.hostname, args.baseline_db, object_key
            );
            let header_target = format!(
                "target:   {}/{} ({})",
                report.host, report.database, object_key
            );

            let Some(patch) =
                diff::render_module_patch(bl_text, tgt_text, &header_baseline, &header_target)
            else {
                continue;
            };

            let filename = format!(
                "{}__{}__{}.diff",
                sanitize(&report.host),
                sanitize(&report.database),
                sanitize(&object_key),
            );
            let path = diff_dir.join(&filename);
            std::fs::write(&path, &patch)
                .with_context(|| format!("Failed to write diff file: {}", path.display()))?;
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
            for r in &reports {
                if r.is_clean {
                    writeln!(out, "{}:{}: no drift detected", r.host, r.database)?;
                } else {
                    writeln!(out, "{}:{}\n{}", r.host, r.database, r.drift)?;
                }
            }
        }
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, &reports)?;
            writeln!(out)?;
        }
    }

    let exit_code = if reports.iter().any(|r| !r.is_clean) {
        1
    } else {
        0
    };
    std::process::exit(exit_code);
}

pub async fn connect(
    host: &ServerHost,
    db_name: &str,
    args: &Args,
) -> anyhow::Result<Client<Compat<TcpStream>>> {
    let mut config = Config::new();

    config.host(host.hostname.clone());
    config.port(host.port.unwrap_or(1433));
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

    Client::connect(config, tcp.compat_write())
        .await
        .context("Failed to connect to database")
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
