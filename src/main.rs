mod diff;
mod fetcher;
mod schema;

use anyhow::Context;
use tiberius::{AuthMethod, Client, Config, Query};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use clap::Parser;


#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[clap(short = 'H', long, default_value = "localhost", env = "SCHEMA_WARDEN_DB_HOST")]
    db_host: String,

    #[clap(short = 'P', long, default_value = "1433", env = "SCHEMA_WARDEN_DB_PORT")]
    db_port: u16,

    #[clap(long, short = 'u', env = "SCHEMA_WARDEN_DB_USER")]
    db_user: String,
    #[clap(long, short = 'p', env = "SCHEMA_WARDEN_DB_PWD", hide_env_values = true)]
    db_pwd: String,

    #[clap(long, short, env = "SCHEMA_WARDEN_BASELINE_DB")]
    baseline_db: String,

    #[clap(long, short, env = "SCHEMA_WARDEN_TRUST_CERT")]
    trust_cert: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();


    let mut baseline_client = connect(&args.baseline_db, &args).await?;
    let baseline = fetcher::fetch_schema(&mut baseline_client, &args.baseline_db).await?;

    let tenants = fetch_tenants(&args).await?;

    for db in tenants {
        if db == args.baseline_db {
            continue;
        }

        let mut client = connect(&db, &args).await?;
        let tenant = fetcher::fetch_schema(&mut client, &db).await?;
        let drift = diff::diff(&baseline, &tenant);

        if drift.is_clean() {
            println!("{db}: no drift detected");
        } else {
            println!("{drift}");
        }
    }

    Ok(())
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
    let mut client = connect("master", &args).await?;

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