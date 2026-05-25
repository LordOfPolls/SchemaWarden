mod fetcher;
mod schema;

use anyhow::Context;
use tiberius::{AuthMethod, Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

const BASELINE_DB: &str = "baseline_db";
const DB_1: &str = "tenant1_db";
const DB_2: &str = "tenant2_db";
const PWD: &str = "SchemaWarden_Dev1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    for db in [BASELINE_DB, DB_1, DB_2] {
        let mut client = connect(db).await?;
        let schema = fetcher::fetch_schema(&mut client, db).await?;

        println!(
            "{}: {} tables, {} views, {} procs, {} functions, {} triggers",
            schema.db_name,
            schema.tables.len(),
            schema.views.len(),
            schema.procedures.len(),
            schema.functions.len(),
            schema.triggers.len(),
        );
    }

    Ok(())
}

pub async fn connect(db_name: &str) -> anyhow::Result<Client<Compat<TcpStream>>> {
    let mut config = Config::new();
    config.host("localhost");
    config.port(1433);
    config.authentication(AuthMethod::sql_server("SA", PWD));
    config.trust_cert();
    config.database(db_name);

    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;

    Client::connect(config, tcp.compat_write())
        .await
        .context("Failed to connect to database")
}
