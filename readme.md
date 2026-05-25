# Schema Warden 
_Catch database drift before your clients do_

When a tenant database in a database drifts, debugging becomes a nightmare. Clients start blowing up your phone.
Schema Warden prevents that.

## What is this?
This is a CLI tool that connects to your database and diffs your schema against a known baseline. 
If there are any differences, they are printed to the console and the process exits with a non-zero exit code.


## Usage

```
Usage: schema-warden [OPTIONS] --db-user <DB_USER> --db-pwd <DB_PWD> --baseline-db <BASELINE_DB>

Options:
  -H, --db-host <DB_HOST>          Hostname or IP address of the SQL Server [env: SCHEMA_WARDEN_DB_HOST=] [default: localhost]
  -P, --db-port <DB_PORT>          SQL Server's TCP port [env: SCHEMA_WARDEN_DB_PORT=] [default: 1433]
  -u, --db-user <DB_USER>          SQL Server login username [env: SCHEMA_WARDEN_DB_USER=]
  -p, --db-pwd <DB_PWD>            SQL Server login password [env: SCHEMA_WARDEN_DB_PWD]
  -b, --baseline-db <BASELINE_DB>  Name of the database to be treated as the source of truth [env: SCHEMA_WARDEN_BASELINE_DB=]
  -t, --trust-cert                 Trust the server's cert without verification [env: SCHEMA_WARDEN_TRUST_CERT=]
  -h, --help                       Print help
  -V, --version                    Print version
```

---

<a href="https://brainmade.org/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://brainmade.org/white-logo.svg">
      <source media="(prefers-color-scheme: light)" srcset="https://brainmade.org/black-logo.svg">
      <img style="float: right;" alt="Brain mark." src="https://brainmade.org/black-logo.svg">
    </picture>
</a>