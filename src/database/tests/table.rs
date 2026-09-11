use crate::{
    catalog::metadata::{ColumnId, DataType, TableId},
    database::{Database, DatabaseError, ExecuteResult},
    test_supports::TestDirectory,
};

use super::users_table;

#[test]
fn create_table은_table_file과_metadata를_생성하고_재시작후에도_유지한다()
-> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-create-table");
    let table_id = {
        let mut database = Database::open(directory.path(), "test")?;
        assert!(matches!(
            database.create_table(&users_table())?,
            ExecuteResult::Success
        ));
        let table_id = TableId::new(1);

        assert!(directory.path().join("1.tbl").exists());
        table_id
    };

    let database = Database::open(directory.path(), "ignored")?;
    let table = database
        .catalog
        .metadata()
        .table("users")
        .expect("재시작 후 users metadata가 있어야 함");

    assert_eq!(table.id(), table_id);
    assert_eq!(table.columns().len(), 2);
    assert_eq!(table.columns()[0].id(), ColumnId::new(1));
    assert_eq!(table.columns()[0].data_type(), DataType::BigInt);
    assert_eq!(table.columns()[1].id(), ColumnId::new(2));
    assert_eq!(table.columns()[1].data_type(), DataType::Varchar);
    Ok(())
}

#[test]
fn create_table은_중복_이름일때_table_file을_남기지_않는다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-duplicate-table");
    let mut database = Database::open(directory.path(), "test")?;

    assert!(matches!(
        database.create_table(&users_table())?,
        ExecuteResult::Success
    ));
    let error = database
        .create_table(&users_table())
        .expect_err("중복 table 이름 오류가 발생해야 함");

    assert!(matches!(
        error,
        DatabaseError::Catalog(crate::catalog::CatalogError::Schema(
            crate::catalog::metadata::SchemaError::DuplicateTableName(name)
        ))
            if name == "users"
    ));
    assert!(!directory.path().join("2.tbl").exists());
    Ok(())
}

#[test]
fn get_or_open_table은_같은_table을_한번만_등록한다() -> Result<(), DatabaseError> {
    let directory = TestDirectory::new("database-table-registry");
    let table_id = {
        let mut database = Database::open(directory.path(), "test")?;
        assert!(matches!(
            database.create_table(&users_table())?,
            ExecuteResult::Success
        ));
        TableId::new(1)
    };

    let mut database = Database::open(directory.path(), "reopened")?;
    {
        let _ = database
            .table_manager
            .get_or_open_table(&mut database.storage_manager, table_id)?;
    }
    assert_eq!(database.table_manager.len(), 1);

    {
        let _ = database
            .table_manager
            .get_or_open_table(&mut database.storage_manager, table_id)?;
    }
    assert_eq!(database.table_manager.len(), 1);

    Ok(())
}
