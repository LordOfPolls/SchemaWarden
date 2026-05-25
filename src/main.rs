use anyhow::Context;
use tiberius::{AuthMethod, Client, Config, Query};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

const BASELINE_DB: &str = "baseline_db";
const DB_1: &str = "tenant1_db";
const DB_2: &str = "tenant2_db";
const pwd: &str = "SchemaWarden_Dev1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Hello, world!");

    let mut client = connect_to_db_server().await?;

    confirm_connection(&mut client).await?;

    Ok(())
}

async fn confirm_connection(client: &mut Client<Compat<TcpStream>>) -> anyhow::Result<()> {
    let select = Query::new("SELECT @@version");

    let stream = select.query(client).await?;
    let row = stream.into_row().await?;

    println!("Server version: {:?}", row.unwrap().get::<&str, _>(0));

    Ok(())
}

async fn connect_to_db_server() -> anyhow::Result<Client<Compat<TcpStream>>> {
    let mut config = Config::new();
    config.host("localhost");
    config.port(1433);
    config.authentication(AuthMethod::sql_server("SA", pwd));
    config.trust_cert();
    config.database(BASELINE_DB);

    let tcp = TcpStream::connect(config.get_addr()).await?;
    tcp.set_nodelay(true)?;

    let client = Client::connect(config, tcp.compat_write()).await
        .context("Failed to connect to database")?;
    Ok(client)
}