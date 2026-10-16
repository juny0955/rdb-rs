use crate::{
    catalog::metadata::{DataType, DatabaseMetadata, TableId},
    database::{Database, DatabaseError, ExecuteResult},
    query::{
        binder::{Binder, BoundColumnDefinition, BoundCreateTable, BoundStatement},
        sql::{Parser, lexer::Lexer},
    },
    storage::page::{PageId, Row, RowId},
    tuple::Value,
};

mod execute;
mod index;
mod join;
mod select_cursor;
mod table;

fn collect_table_rows(
    database: &mut Database,
    table_id: TableId,
) -> Result<Vec<(RowId, Row)>, DatabaseError> {
    let page_count = database
        .table_manager
        .page_count(&mut database.storage_manager, table_id)?;
    let mut rows = Vec::new();
    for page_id in 0..page_count {
        rows.extend(database.table_manager.scan_page_rows(
            &mut database.storage_manager,
            table_id,
            PageId::new(page_id),
        )?);
    }
    Ok(rows)
}

fn collect_select_rows(result: ExecuteResult<'_>) -> Result<Vec<Vec<Value>>, DatabaseError> {
    let ExecuteResult::Rows(mut cursor) = result else {
        panic!("SELECT는 행 커서를 반환해야 함");
    };

    let mut rows = Vec::new();
    while let Some(row) = cursor.next_row()? {
        rows.push(row);
    }
    Ok(rows)
}

fn users_table() -> BoundCreateTable {
    BoundCreateTable {
        table: "users".to_owned(),
        columns: vec![
            BoundColumnDefinition {
                name: "id".to_owned(),
                data_type: DataType::BigInt,
            },
            BoundColumnDefinition {
                name: "name".to_owned(),
                data_type: DataType::Varchar,
            },
        ],
    }
}

fn bind_sql(sql: &str, metadata: &DatabaseMetadata) -> BoundStatement {
    let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
    let statement = Parser::new(tokens).parse().expect("SQL을 파싱해야 함");
    Binder::new(metadata)
        .bind(&statement)
        .expect("SQL을 bind해야 함")
}

fn parse_sql(sql: &str) -> crate::query::sql::ast::Statement {
    let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
    Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
}
