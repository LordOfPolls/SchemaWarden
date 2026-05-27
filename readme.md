# Schema Warden
_Catch database drift before your clients do_

When a tenant database drifts from your baseline, debugging becomes a nightmare. Clients start blowing up your phone.
Schema Warden prevents that.

## What is this?
A CLI tool that connects to one or more **MSSQL** servers, diffs every tenant database against a known-good baseline, and reports anything that doesn't match. 
Non-zero exit if drift is found, so it can slot straight into CI or a scheduled job.

It compares tables (columns, indexes, foreign keys, check constraints) and modules (views, stored procedures, functions, triggers). 
For module drift you can ask it to dump unified `.diff` files so you can read the SQL change in your editor of choice.

> [!NOTE]
> Schema Warden opens its connections with `ApplicationIntent=ReadOnly`. The server itself will reject any write, so the tool physically cannot mutate your databases, even if you wanted to.

---

## Installation

This assumes you already have a working Rust toolchain.
```bash
git clone https://github.com/LordOfPolls/SchemaWarden.git
cd SchemaWarden
cargo build --release
```

The binary lands at `target/release/schema-warden`.

**Alternatively**, the [releases](https://github.com/LordOfPolls/SchemaWarden/releases) contain a pre-built binary for Linux and Windows.

## Usage

```
Usage: schema-warden [OPTIONS] --db-user <DB_USER> --db-password <DB_PWD> --baseline-db <BASELINE_DB>

Options:
  -H, --db-host <DB_HOST>
          SQL Server host. Repeat for multiple hosts. Use host:port for non-default ports (e.g. myserver:1435) [env: SCHEMA_WARDEN_DB_HOST=] [default: localhost]
  -u, --db-user <DB_USER>
          SQL Server login username [env: SCHEMA_WARDEN_DB_USER=]
  -p, --db-password <DB_PWD>
          SQL Server login password [env: SCHEMA_WARDEN_DB_PWD]
  -b, --baseline-db <BASELINE_DB>
          Name of the database to be treated as the source of truth [env: SCHEMA_WARDEN_BASELINE_DB=]
      --baseline-host <BASELINE_HOST>
          Baseline database host, defaults to the first db-host [env: SCHEMA_WARDEN_BASELINE_HOST=]
  -e, --exclude-databases <EXCLUDE_DATABASES>
          Databases to exclude. Comma-separated or repeated flags: -e db1,db2 or -e db1 -e db2 [env: SCHEMA_WARDEN_EXCLUDE_DATABASES=]
  -t, --trust-cert
          Trust the server's cert without verification [env: SCHEMA_WARDEN_TRUST_CERT=]
      --object <OBJECT>
          Limit diff to a specific object. Format: [schema.]name — defaults to dbo if schema is omitted [env: SCHEMA_WARDEN_OBJECT=]
  -c, --concurrency <CONCURRENCY>
          Maximum number of tenant databases to scan in parallel [env: SCHEMA_WARDEN_CONCURRENCY=] [default: 4]
      --format <FORMAT>
          Output format [default: json] [possible values: text, json]
  -o, --output <OUTPUT>
          Write output to this file instead of stdout [env: SCHEMA_WARDEN_OUTPUT=]
      --diff-dir <DIFF_DIR>
          Write a unified-diff file per drifted tenant. Requires --object pointing at a module-type object [env: SCHEMA_WARDEN_DIFF_DIR=]
  -h, --help
          Print help
  -V, --version
          Print version
```

Most flags have an environment variable equivalent. 
A `.env` file in the working directory is auto-loaded.

## Examples

**Scan every tenant on a single server:**
```bash
schema-warden -H sql.example.com -u svc_warden -p '...' -b BaselineDB
```

**Scan a whole fleet:**
```bash
schema-warden -H sql-01,sql-02,sql-03 -u svc_warden -p '...' -b BaselineDB
```
Each host is queried for its tenant list independently. The baseline lives on the first host unless `--baseline-host` says otherwise.

**Baseline on a separate server:**
```bash
schema-warden \
  -H sql-prod-01,sql-prod-02 \
  --baseline-host sql-reference \
  -b BaselineDB \
  -u svc_warden -p '...'
```

**Focus on one object across every tenant:**
```bash
schema-warden ... --object dbo.Invoices
```
Useful when you've just shipped a migration and want to confirm it landed everywhere.

**Dump per-tenant patch files for a drifted view or procedure:**
```bash
schema-warden ... --object dbo.usp_BillRun --diff-dir ./drift-patches
```
Each drifted tenant gets a `host__database__schema.object.diff` file containing a unified diff between baseline and tenant. Open it in your editor; apply it with `patch` if you trust it.

> [!NOTE]
> `--diff-dir` only works for modules (views, procedures, functions, triggers) — tables don't have a single body of SQL to diff.

## Output

Default output is JSON, suitable for piping into `jq` or storing as a CI artifact:

```bash
schema-warden ... | jq '.[] | select(.is_clean == false) | .database'
```


```
sql-01:TenantA: no drift detected
sql-01:TenantB
  Tables:
    ~ dbo.Invoices
        + column "Notes" (nvarchar(max), nullable)
  Procedures:
    ~ dbo.usp_BillRun (definition changed)
```

Schema Warden exits `0` if every tenant matches the baseline, `1` if any drift was found, and non-zero on connection or query errors. 
Hook that into your CI step and you'll get a failed build the moment something diverges.

---

<a href="https://brainmade.org/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://brainmade.org/white-logo.svg">
      <source media="(prefers-color-scheme: light)" srcset="https://brainmade.org/black-logo.svg">
      <img style="float: right;" alt="Brain mark." src="https://brainmade.org/black-logo.svg">
    </picture>
</a>
