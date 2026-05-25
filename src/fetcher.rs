use anyhow::Context;
use indexmap::IndexMap;
use sqlparser::parser::Parser;
use sqlparser::dialect::MsSqlDialect;
use tiberius::{Client, Query};
use tokio::net::TcpStream;
use tokio_util::compat::Compat;
use regex;

use crate::schema::*;

const DIALECT: MsSqlDialect = MsSqlDialect {};
static AUTO_GEN_INDEX_REG: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^PK__.+__[A-F0-9]{16}$").unwrap()
});

pub async fn fetch_schema(
    client: &mut Client<Compat<TcpStream>>,
    db_name: &str,
    filter: Option<(&str, &str)>,
) -> anyhow::Result<DatabaseSchema> {
    let mut schema = DatabaseSchema::new(db_name);

    fetch_columns(client, &mut schema.tables, filter)
        .await
        .context("fetching columns")?;
    fetch_indexes(client, &mut schema.tables, filter)
        .await
        .context("fetching indexes")?;
    fetch_foreign_keys(client, &mut schema.tables, filter)
        .await
        .context("fetching foreign keys")?;
    fetch_check_constraints(client, &mut schema.tables, filter)
        .await
        .context("fetching check constraints")?;
    fetch_sql_modules(client, &mut schema, filter)
        .await
        .context("fetching SQL modules")?;

    Ok(schema)
}

async fn fetch_columns(
    client: &mut Client<Compat<TcpStream>>,
    tables: &mut IndexMap<String, TableDef>,
    filter: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    let where_clause = if filter.is_some() {
        "WHERE c.TABLE_SCHEMA = @P1 AND c.TABLE_NAME = @P2"
    } else {
        ""
    };
    let sql = format!("
        SELECT c.TABLE_SCHEMA
            ,c.TABLE_NAME
            ,c.COLUMN_NAME
            ,c.ORDINAL_POSITION
            ,c.DATA_TYPE
            ,c.CHARACTER_MAXIMUM_LENGTH
            ,c.NUMERIC_PRECISION
            ,c.NUMERIC_SCALE
            ,c.IS_NULLABLE
            ,c.COLUMN_DEFAULT
            ,sc.is_identity
        FROM INFORMATION_SCHEMA.COLUMNS c
        INNER JOIN sys.columns sc ON sc.object_id = OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA) + '.' + QUOTENAME(c.TABLE_NAME))
            AND sc.name = c.COLUMN_NAME
        INNER JOIN sys.tables st ON st.object_id = sc.object_id
        {where_clause}
        ORDER BY c.TABLE_SCHEMA
            ,c.TABLE_NAME
            ,c.ORDINAL_POSITION
    ");

    let mut query = Query::new(sql);
    if let Some((schema, name)) = filter {
        query.bind(schema);
        query.bind(name);
    }
    let rows = query
        .query(client)
        .await?
        .into_first_result()
        .await?;

    for row in rows {
        let table_schema = row.get::<&str, _>(0).unwrap_or("").to_owned();
        let table_name = row.get::<&str, _>(1).unwrap_or("").to_owned();
        let column_name = row.get::<&str, _>(2).unwrap_or("").to_owned();
        let ordinal: i32 = row.get(3).unwrap_or(0);
        let data_type = row.get::<&str, _>(4).unwrap_or("").to_owned();
        let max_length: Option<i32> = row.get(5);
        let precision: Option<u8> = row.get(6);
        let scale: Option<i32> = row.get(7);
        let is_nullable = row.get::<&str, _>(8).unwrap_or("NO") == "YES";
        let default_value: Option<String> = row.get::<&str, _>(9).map(str::to_owned);
        let is_identity: bool = row.get(10).unwrap_or(false);

        let key = format!("{}.{}", table_schema, table_name);
        let table = tables
            .entry(key)
            .or_insert_with(|| TableDef::new(table_schema, table_name));

        table.columns.push(ColumnDef {
            name: column_name,
            ordinal,
            data_type,
            max_length,
            precision,
            scale,
            is_nullable,
            default_value,
            is_identity,
        });
    }

    Ok(())
}

async fn fetch_indexes(
    client: &mut Client<Compat<TcpStream>>,
    tables: &mut IndexMap<String, TableDef>,
    filter: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    let filter_clause = if filter.is_some() {
        "AND s.name = @P1 AND t.name = @P2"
    } else {
        ""
    };
    let sql = format!("
        SELECT s.name AS schema_name
            ,t.name AS table_name
            ,i.name AS index_name
            ,i.is_unique
            ,i.is_primary_key
            ,CAST(CASE
                    WHEN i.type = 1
                        THEN 1
                    ELSE 0
                    END AS BIT) AS is_clustered
            ,c.name AS column_name
            ,ic.is_descending_key
            ,ic.is_included_column
        FROM sys.indexes i
        INNER JOIN sys.tables t ON t.object_id = i.object_id
        INNER JOIN sys.schemas s ON s.schema_id = t.schema_id
        INNER JOIN sys.index_columns ic ON ic.object_id = i.object_id
            AND ic.index_id = i.index_id
        INNER JOIN sys.columns c ON c.object_id = ic.object_id
            AND c.column_id = ic.column_id
        WHERE i.name IS NOT NULL
            AND i.type > 0
            {filter_clause}
        ORDER BY s.name
            ,t.name
            ,i.name
            ,ic.key_ordinal
    ");

    let mut query = Query::new(sql);
    if let Some((schema, name)) = filter {
        query.bind(schema);
        query.bind(name);
    }
    let rows = query
        .query(client)
        .await?
        .into_first_result()
        .await?;

    for row in rows {
        let schema_name = row.get::<&str, _>(0).unwrap_or("");
        let table_name = row.get::<&str, _>(1).unwrap_or("");
        let key = format!("{}.{}", schema_name, table_name);

        let index_name = row.get::<&str, _>(2).unwrap_or("").to_owned();
        let is_unique: bool = row.get(3).unwrap_or(false);
        let is_primary_key: bool = row.get(4).unwrap_or(false);
        let is_clustered: bool = row.get(5).unwrap_or(false);

        // PK__TableName_ID is an autogenerated index for primary keys, its name will be different for each db
        if is_primary_key && index_name.starts_with("PK__") && AUTO_GEN_INDEX_REG.is_match(&index_name) {
            continue;
        }

        let col_ref = IndexColumnRef {
            name: row.get::<&str, _>(6).unwrap_or("").to_owned(),
            is_descending: row.get::<bool, _>(7).unwrap_or(false),
            is_included: row.get::<bool, _>(8).unwrap_or(false),
        };

        let Some(table) = tables.get_mut(&key) else {
            continue;
        };

        if let Some(idx) = table.indexes.iter_mut().find(|i| i.name == index_name) {
            idx.columns.push(col_ref);
        } else {
            table.indexes.push(IndexDef {
                name: index_name,
                is_unique,
                is_primary_key,
                is_clustered,
                columns: vec![col_ref],
            });
        }
    }

    Ok(())
}

async fn fetch_foreign_keys(
    client: &mut Client<Compat<TcpStream>>,
    tables: &mut IndexMap<String, TableDef>,
    filter: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    let where_clause = if filter.is_some() {
        "WHERE s.name = @P1 AND t.name = @P2"
    } else {
        ""
    };
    let sql = format!("
        SELECT s.name AS schema_name
            ,t.name AS table_name
            ,fk.name AS fk_name
            ,c.name AS column_name
            ,rs.name AS ref_schema
            ,rt.name AS ref_table
            ,rc.name AS ref_column
            ,fk.delete_referential_action_desc
            ,fk.update_referential_action_desc
        FROM sys.foreign_keys fk
        INNER JOIN sys.tables t ON t.object_id = fk.parent_object_id
        INNER JOIN sys.schemas s ON s.schema_id = t.schema_id
        INNER JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id
        INNER JOIN sys.columns c ON c.object_id = fkc.parent_object_id
            AND c.column_id = fkc.parent_column_id
        INNER JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id
        INNER JOIN sys.schemas rs ON rs.schema_id = rt.schema_id
        INNER JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id
            AND rc.column_id = fkc.referenced_column_id
        {where_clause}
        ORDER BY s.name
            ,t.name
            ,fk.name
            ,fkc.constraint_column_id
    ");

    let mut query = Query::new(sql);
    if let Some((schema, name)) = filter {
        query.bind(schema);
        query.bind(name);
    }
    let rows = query
        .query(client)
        .await?
        .into_first_result()
        .await?;

    for row in rows {
        let schema_name = row.get::<&str, _>(0).unwrap_or("");
        let table_name = row.get::<&str, _>(1).unwrap_or("");
        let key = format!("{}.{}", schema_name, table_name);

        let fk_name = row.get::<&str, _>(2).unwrap_or("").to_owned();
        let column = row.get::<&str, _>(3).unwrap_or("").to_owned();
        let ref_schema = row.get::<&str, _>(4).unwrap_or("").to_owned();
        let ref_table = row.get::<&str, _>(5).unwrap_or("").to_owned();
        let ref_column = row.get::<&str, _>(6).unwrap_or("").to_owned();
        let on_delete = row.get::<&str, _>(7).unwrap_or("").to_owned();
        let on_update = row.get::<&str, _>(8).unwrap_or("").to_owned();

        let Some(table) = tables.get_mut(&key) else {
            continue;
        };

        if let Some(fk) = table.foreign_keys.iter_mut().find(|f| f.name == fk_name) {
            fk.columns.push(column);
            fk.ref_columns.push(ref_column);
        } else {
            table.foreign_keys.push(ForeignKeyDef {
                name: fk_name,
                columns: vec![column],
                ref_schema,
                ref_table,
                ref_columns: vec![ref_column],
                on_delete,
                on_update,
            });
        }
    }

    Ok(())
}

async fn fetch_check_constraints(
    client: &mut Client<Compat<TcpStream>>,
    tables: &mut IndexMap<String, TableDef>,
    filter: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    let where_clause = if filter.is_some() {
        "WHERE s.name = @P1 AND t.name = @P2"
    } else {
        ""
    };
    let sql = format!("
        SELECT s.name AS schema_name
            ,t.name AS table_name
            ,cc.name AS constraint_name
            ,cc.DEFINITION
        FROM sys.check_constraints cc
        INNER JOIN sys.tables t ON t.object_id = cc.parent_object_id
        INNER JOIN sys.schemas s ON s.schema_id = t.schema_id
        {where_clause}
        ORDER BY s.name
            ,t.name
            ,cc.name
    ");

    let mut query = Query::new(sql);
    if let Some((schema, name)) = filter {
        query.bind(schema);
        query.bind(name);
    }
    let rows = query
        .query(client)
        .await?
        .into_first_result()
        .await?;

    for row in rows {
        let schema_name = row.get::<&str, _>(0).unwrap_or("");
        let table_name = row.get::<&str, _>(1).unwrap_or("");
        let key = format!("{}.{}", schema_name, table_name);

        let Some(table) = tables.get_mut(&key) else {
            continue;
        };

        table.check_constraints.push(CheckConstraintDef {
            name: row.get::<&str, _>(2).unwrap_or("").to_owned(),
            definition: row.get::<&str, _>(3).unwrap_or("").to_owned(),
        });
    }

    Ok(())
}

async fn fetch_sql_modules(
    client: &mut Client<Compat<TcpStream>>,
    schema: &mut DatabaseSchema,
    filter: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    let filter_clause = if filter.is_some() {
        "AND s.name = @P1 AND o.name = @P2"
    } else {
        ""
    };
    let sql = format!("
        SELECT s.name AS schema_name
            ,o.name AS object_name
            ,o.type_desc
            ,sm.DEFINITION
        FROM sys.sql_modules sm
        INNER JOIN sys.objects o ON o.object_id = sm.object_id
        INNER JOIN sys.schemas s ON s.schema_id = o.schema_id
        WHERE o.type IN (
                'V'
                ,'P'
                ,'FN'
                ,'IF'
                ,'TF'
                ,'TR'
                )
            {filter_clause}
        ORDER BY o.type
            ,s.name
            ,o.name
    ");

    let mut query = Query::new(sql);
    if let Some((sch, name)) = filter {
        query.bind(sch);
        query.bind(name);
    }
    let rows = query
        .query(client)
        .await?
        .into_first_result()
        .await?;

    for row in rows {
        let schema_name = row.get::<&str, _>(0).unwrap_or("").to_owned();
        let object_name = row.get::<&str, _>(1).unwrap_or("").to_owned();
        let type_desc = row.get::<&str, _>(2).unwrap_or("");
        let mut definition = row.get::<&str, _>(3).unwrap_or("").to_owned();

        let Some(object_type) = ObjectType::from_type_desc(type_desc) else {
            continue;
        };

        let key = format!("{}.{}", schema_name, object_name);

        let try_ast = Parser::parse_sql(&DIALECT, &definition);

        // Should normalise defs for whitespace and comments
        // if parser fails, try a dumber cleanup
        if let Ok(ast) = try_ast {
           definition = ast.iter().map(ToString::to_string).collect::<Vec<_>>().join(";\n");
        }else{
            definition = definition.lines()
                .map(str::trim)
                .filter(|line| !line.starts_with("--") && !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }

        let module = ModuleDef {
            schema: schema_name,
            name: object_name,
            object_type: object_type.clone(),
            definition,
        };

        match object_type {
            ObjectType::View => {
                schema.views.insert(key, module);
            }
            ObjectType::StoredProcedure => {
                schema.procedures.insert(key, module);
            }
            ObjectType::ScalarFunction
            | ObjectType::InlineTableValuedFunction
            | ObjectType::MultiStatementTableValuedFunction => {
                schema.functions.insert(key, module);
            }
            ObjectType::Trigger => {
                schema.triggers.insert(key, module);
            }
        }
    }

    Ok(())
}
