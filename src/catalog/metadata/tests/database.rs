use crate::{
    catalog::metadata::{
        ColumnId, ColumnMetadata, DataType, DatabaseMetadata, IndexId, IndexMetadata, SchemaError,
        TableId, TableMetadata,
    },
    storage::page::PageId,
};

fn table(id: u32, name: &str) -> Result<TableMetadata, SchemaError> {
    TableMetadata::new(
        TableId::new(id),
        name.to_owned(),
        vec![ColumnMetadata::new(
            ColumnId::new(1),
            "id".to_owned(),
            DataType::BigInt,
        )],
    )
}

#[test]
fn duplicate_table_name_테스트() -> Result<(), SchemaError> {
    let columns1 = vec![
        ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar),
        ColumnMetadata::new(ColumnId::new(2), "address".to_string(), DataType::Varchar),
    ];
    let columns2 = vec![
        ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar),
        ColumnMetadata::new(ColumnId::new(2), "address".to_string(), DataType::Varchar),
    ];

    let tables = vec![
        TableMetadata::new(TableId::new(1), "users".to_string(), columns1)?,
        TableMetadata::new(TableId::new(2), "users".to_string(), columns2)?,
    ];
    let error =
        DatabaseMetadata::new("mydb".to_string(), tables, vec![]).expect_err("에러 발생해야함");
    assert_eq!(error, SchemaError::DuplicateTableName("users".to_string()));

    Ok(())
}

#[test]
fn duplicate_table_id_테스트() -> Result<(), SchemaError> {
    let users = TableMetadata::new(
        TableId::new(1),
        "users".to_string(),
        vec![ColumnMetadata::new(
            ColumnId::new(1),
            "name".to_string(),
            DataType::Varchar,
        )],
    )?;
    let orders = TableMetadata::new(
        TableId::new(1),
        "orders".to_string(),
        vec![ColumnMetadata::new(
            ColumnId::new(1),
            "amount".to_string(),
            DataType::Int,
        )],
    )?;

    let error = DatabaseMetadata::new("mydb".to_string(), vec![users, orders], vec![])
        .expect_err("에러 발생해야함");

    assert_eq!(error, SchemaError::DuplicateTableId(TableId::new(1)));
    Ok(())
}

#[test]
fn add_table은_새_테이블을_추가한다() -> Result<(), SchemaError> {
    let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![], vec![])?;

    database.add_table(table(1, "users")?)?;

    assert_eq!(database.tables().len(), 1);
    assert_eq!(
        database.table_by_id(TableId::new(1)).unwrap().name(),
        "users"
    );
    Ok(())
}

#[test]
fn add_table은_중복_id를_거부하고_변경하지_않는다() -> Result<(), SchemaError> {
    let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![])?;

    let error = database
        .add_table(table(1, "orders")?)
        .expect_err("중복 TableId 오류가 발생해야 함");

    assert_eq!(error, SchemaError::DuplicateTableId(TableId::new(1)));
    assert_eq!(database.tables().len(), 1);
    assert!(database.table("orders").is_none());
    Ok(())
}

#[test]
fn add_table은_중복_이름을_거부하고_변경하지_않는다() -> Result<(), SchemaError> {
    let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![])?;

    let error = database
        .add_table(table(2, "users")?)
        .expect_err("중복 table 이름 오류가 발생해야 함");

    assert_eq!(error, SchemaError::DuplicateTableName("users".to_owned()));
    assert_eq!(database.tables().len(), 1);
    assert!(database.table_by_id(TableId::new(2)).is_none());
    Ok(())
}

#[test]
fn new는_없는_테이블을_참조하는_index를_거부한다() -> Result<(), SchemaError> {
    let index = IndexMetadata::new(
        IndexId::new(1),
        "idx_users_id".to_owned(),
        TableId::new(999),
        ColumnId::new(1),
        PageId::new(0),
    );

    let error = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![index])
        .expect_err("존재하지 않는 인덱스 대상 테이블은 거부해야 함");

    assert_eq!(error, SchemaError::IndexTableNotFound(TableId::new(999)));
    Ok(())
}

#[test]
fn new는_없는_컬럼을_참조하는_index를_거부한다() -> Result<(), SchemaError> {
    let index = IndexMetadata::new(
        IndexId::new(1),
        "idx_users_unknown".to_owned(),
        TableId::new(1),
        ColumnId::new(999),
        PageId::new(0),
    );

    let error = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![index])
        .expect_err("존재하지 않는 인덱스 대상 컬럼은 거부해야 함");

    assert_eq!(
        error,
        SchemaError::IndexColumnNotFound {
            table_id: TableId::new(1),
            column_id: ColumnId::new(999),
        }
    );
    Ok(())
}

#[test]
fn add_index는_없는_테이블을_거부하고_추가하지_않는다() -> Result<(), SchemaError> {
    let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![])?;
    let index = IndexMetadata::new(
        IndexId::new(1),
        "idx_missing_table".to_owned(),
        TableId::new(999),
        ColumnId::new(1),
        PageId::new(0),
    );

    let error = database
        .add_index(index)
        .expect_err("존재하지 않는 인덱스 대상 테이블은 거부해야 함");

    assert_eq!(error, SchemaError::IndexTableNotFound(TableId::new(999)));
    assert!(database.index_by_id(IndexId::new(1)).is_none());
    Ok(())
}

#[test]
fn 직렬화_역직렬화_테스트() -> Result<(), SchemaError> {
    let column = ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar);
    let table = TableMetadata::new(TableId::new(1), "users".to_string(), vec![column])?;
    let database = DatabaseMetadata::new("mydb".to_string(), vec![table], vec![])?;
    let bytes = database.to_bytes()?;
    assert_eq!(database, DatabaseMetadata::from_bytes(&bytes)?.0);
    Ok(())
}
