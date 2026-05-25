mod diff;
mod fetcher;
mod schema;

use anyhow::Context;
use tiberius::{AuthMethod, Client, Config};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

const BASELINE_DB: &str = "baseline_db";
const TENANT_DBS: &[&str] = &["tenant1_db", "tenant2_db"];
const PWD: &str = "SchemaWarden_Dev1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut baseline_client = connect(BASELINE_DB).await?;
    let baseline = fetcher::fetch_schema(&mut baseline_client, BASELINE_DB).await?;

    for &db in TENANT_DBS {
        let mut client = connect(db).await?;
        let tenant = fetcher::fetch_schema(&mut client, db).await?;
        let drift = diff::diff(&baseline, &tenant);

        if drift.is_clean() {
            println!("{db}: no drift detected");
        } else {
            println!("{drift}");
        }
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
