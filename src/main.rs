mod diff;
mod fetcher;
mod schema;

use anyhow::Context;
use futures::stream::{self, StreamExt, TryStreamExt};
use tiberius::{AuthMethod, Client, Config, Query};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use clap::Parser;


#[derive(Parser, Debug)]
#[command(version, about = "Schema drift detection for multi-tenant MSSQL databases")]
pub struct Args {
    #[clap(short = 'H', long, default_value = "localhost", env = "SCHEMA_WARDEN_DB_HOST",
        help = "Hostname or IP address of the SQL Server")]
    db_host: String,

    #[clap(short = 'P', long, default_value = "1433", env = "SCHEMA_WARDEN_DB_PORT",
        help = "SQL Server's TCP port")]
    db_port: u16,

    #[clap(long, short = 'u', env = "SCHEMA_WARDEN_DB_USER",
        help = "SQL Server login username")]
    db_user: String,

    #[clap(long, short = 'p', env = "SCHEMA_WARDEN_DB_PWD", hide_env_values = true,
        help = "SQL Server login password")]
    db_pwd: String,

    #[clap(long, short, env = "SCHEMA_WARDEN_BASELINE_DB",
        help = "Name of the database to be treated as the source of truth")]
    baseline_db: String,

    #[clap(long, short, env = "SCHEMA_WARDEN_EXCLUDE_DATABASES",
        help = "Database to exclude from the comparison",
    )]
    exclude_databases: Vec<String>,

    #[clap(long, short, env = "SCHEMA_WARDEN_TRUST_CERT",
        help = "Trust the server's cert without verification")]
    trust_cert: bool,

    #[clap(long, short, env = "SCHEMA_WARDEN_OBJECT",
        help = "Limit diff to a single object")]
    object: Option<String>,

    #[clap(long, short = 'c', default_value_t = 4, env = "SCHEMA_WARDEN_CONCURRENCY",
        help = "Maximum number of tenant databases to scan in parallel")]
    concurrency: usize,
}

fn parse_object_filter(s: &str) -> (String, String) {
    match s.split_once('.') {
        Some((schema, name)) => (schema.to_owned(), name.to_owned()),
        None => ("dbo".to_owned(), s.to_owned()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let filter = args.object.as_deref().map(parse_object_filter);
    let filter_ref = filter.as_ref().map(|(s, n)| (s.as_str(), n.as_str()));

    let mut baseline_client = connect(&args.baseline_db, &args).await?;
    let baseline = fetcher::fetch_schema(&mut baseline_client, &args.baseline_db, filter_ref).await?;

    let tenants = fetch_tenants(&args).await?;
    let concurrency = args.concurrency.max(1);

    let drifts: Vec<bool> = stream::iter(
        tenants
            .into_iter()
            .filter(|db| db != &args.baseline_db && !args.exclude_databases.contains(db)),
    )
    .map(|db| {
        let baseline = &baseline;
        let args = &args;
        async move {
            let mut client = connect(&db, args).await?;
            let tenant = fetcher::fetch_schema(&mut client, &db, filter_ref).await?;
            let drift = diff::diff(baseline, &tenant);

            if drift.is_clean() {
                println!("{db}: no drift detected");
                Ok::<bool, anyhow::Error>(false)
            } else {
                println!("{drift}");
                Ok::<bool, anyhow::Error>(true)
            }
        }
    })
    .buffer_unordered(concurrency)
    .try_collect()
    .await?;

    let exit_code = if drifts.iter().any(|&d| d) { 1 } else { 0 };
    std::process::exit(exit_code);
}

pub async fn connect(db_name: &str, args: &Args) -> anyhow::Result<Client<Compat<TcpStream>>> {
    let mut config = Config::new();
    config.host(args.db_host.clone());
    config.port(args.db_port);
    config.authentication(AuthMethod::sql_server(args.db_user.clone(), args.db_pwd.clone()));

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


pub async fn fetch_tenants(args: &Args) -> anyhow::Result<Vec<String>> {
    let mut client = connect("master", args).await?;

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
        .await
        ?.into_first_result()
        .await;

    let mut tenants = Vec::new();
    for row in rows? {
        let name = row.get::<&str, _>(0).unwrap_or("").to_owned();
        tenants.push(name);
    }

    Ok(tenants)
}