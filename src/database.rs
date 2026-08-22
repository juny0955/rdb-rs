use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{
    binder::BoundCreateTable,
    catalog::{Catalog, CatalogError},
    schema::{ColumnId, ColumnMetadata, DatabaseMetadata, SchemaError, TableId, TableMetadata},
    table::HeapTable,
};

#[derive(Debug)]
pub enum DatabaseError {
    Catalog(CatalogError),
    Schema(SchemaError),
    Io(io::Error),
}

impl From<CatalogError> for DatabaseError {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}

impl From<SchemaError> for DatabaseError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

impl From<io::Error> for DatabaseError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub struct Database {
    metadata: DatabaseMetadata,
    catalog: Catalog,
    data_dir: PathBuf,
}

impl Database {
    pub fn open(data_dir: &Path, name: &str) -> Result<Self, DatabaseError> {
        let mut catalog = Catalog::open(&data_dir.join("catalog"))?;

        let metadata = match catalog.load() {
            Ok(metadata) => metadata,
            Err(CatalogError::EmptyCatalog) => {
                let metadata = DatabaseMetadata::new(name.to_owned(), vec![])?;
                catalog.save(&metadata)?;
                metadata
            }
            Err(e) => return Err(e.into()),
        };

        Ok(Self {
            metadata,
            catalog,
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn create_table(&mut self, bound: &BoundCreateTable) -> Result<TableId, DatabaseError> {
        if self.metadata.tables().len() >= usize::from(u16::MAX) {
            return Err(DatabaseError::Schema(SchemaError::TooManyTables));
        }
        let table_id = TableId::new(self.metadata.tables().len() as u32 + 1);

        let mut columns = Vec::new();
        for (index, column) in (1..).zip(&bound.columns) {
            let column_id = ColumnId::new(index);
            columns.push(ColumnMetadata::new(
                column_id,
                column.name.to_owned(),
                column.data_type.clone().into(),
            ));
        }

        let table = TableMetadata::new(table_id, bound.table.to_owned(), columns)?;
        if self.metadata.table(&bound.table).is_some() {
            return Err(DatabaseError::Schema(SchemaError::DuplicateTableName(
                bound.table.to_owned(),
            )));
        }

        let _ = HeapTable::open(&self.data_dir.join(format!("{}.tbl", table_id.id())))?;
        self.metadata.add_table(table)?;
        self.catalog.save(&self.metadata)?;

        Ok(table_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        binder::{Binder, BoundCreateTable, BoundStatement},
        executor::Executor,
        parser::{
            Parser,
            ast::{ColumnDefinition, DataType as AstDataType},
            lexer::Lexer,
        },
        schema::DataType,
        test_supports::TestDirectory,
        tuple::Value,
    };

    use super::*;

    fn users_table() -> BoundCreateTable {
        BoundCreateTable {
            table: "users".to_owned(),
            columns: vec![
                ColumnDefinition {
                    name: "id".to_owned(),
                    data_type: AstDataType::BigInt,
                },
                ColumnDefinition {
                    name: "name".to_owned(),
                    data_type: AstDataType::Varchar,
                },
            ],
        }
    }

    #[test]
    fn create_table은_table_file과_metadata를_생성하고_재시작후에도_유지한다()
    -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-create-table");
        let table_id = {
            let mut database = Database::open(directory.path(), "test")?;
            let table_id = database.create_table(&users_table())?;

            assert_eq!(table_id, TableId::new(1));
            assert!(directory.path().join("1.tbl").exists());
            table_id
        };

        let database = Database::open(directory.path(), "ignored")?;
        let table = database
            .metadata
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

        database.create_table(&users_table())?;
        let error = database
            .create_table(&users_table())
            .expect_err("중복 table 이름 오류가 발생해야 함");

        assert!(matches!(
            error,
            DatabaseError::Schema(SchemaError::DuplicateTableName(name)) if name == "users"
        ));
        assert!(!directory.path().join("2.tbl").exists());
        Ok(())
    }

    fn bind_sql(sql: &str, metadata: &DatabaseMetadata) -> BoundStatement {
        let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
        let statement = Parser::new(tokens).parse().expect("SQL을 파싱해야 함");
        Binder::new(metadata)
            .bind(&statement)
            .expect("SQL을 bind해야 함")
    }

    #[test]
    fn sql_crud는_parser부터_file까지_동작한다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("sql-crud-integration");
        let mut database = Database::open(directory.path(), "test")?;

        let BoundStatement::CreateTable(bound) = bind_sql(
            "CREATE TABLE users (id BIGINT, name VARCHAR);",
            &database.metadata,
        ) else {
            panic!("CREATE TABLE이 bind되어야 함");
        };
        database.create_table(&bound)?;

        let BoundStatement::Insert(bound) =
            bind_sql("INSERT INTO users VALUES (1, 'Kim');", &database.metadata)
        else {
            panic!("INSERT가 bind되어야 함");
        };
        Executor::new(&database.metadata, &database.data_dir)
            .execute_insert(&bound)
            .expect("INSERT가 실행되어야 함");

        let BoundStatement::Select(bound) =
            bind_sql("SELECT name FROM users WHERE id = 1;", &database.metadata)
        else {
            panic!("SELECT가 bind되어야 함");
        };
        let rows = Executor::new(&database.metadata, &database.data_dir)
            .execute_select(&bound)
            .expect("SELECT가 실행되어야 함");
        assert_eq!(rows, vec![vec![Value::Varchar("Kim".to_owned())]]);

        let BoundStatement::Update(bound) = bind_sql(
            "UPDATE users SET name = 'Lee' WHERE id = 1;",
            &database.metadata,
        ) else {
            panic!("UPDATE가 bind되어야 함");
        };
        let updated = Executor::new(&database.metadata, &database.data_dir)
            .execute_update(&bound)
            .expect("UPDATE가 실행되어야 함");
        assert_eq!(updated, 1);

        let BoundStatement::Select(bound) = bind_sql("SELECT * FROM users;", &database.metadata)
        else {
            panic!("SELECT가 bind되어야 함");
        };
        let rows = Executor::new(&database.metadata, &database.data_dir)
            .execute_select(&bound)
            .expect("SELECT가 실행되어야 함");
        assert_eq!(
            rows,
            vec![vec![Value::BigInt(1), Value::Varchar("Lee".to_owned())]]
        );

        let BoundStatement::Delete(bound) =
            bind_sql("DELETE FROM users WHERE id = 1;", &database.metadata)
        else {
            panic!("DELETE가 bind되어야 함");
        };
        let deleted = Executor::new(&database.metadata, &database.data_dir)
            .execute_delete(&bound)
            .expect("DELETE가 실행되어야 함");
        assert_eq!(deleted, 1);

        let BoundStatement::Select(bound) = bind_sql("SELECT * FROM users;", &database.metadata)
        else {
            panic!("SELECT가 bind되어야 함");
        };
        let rows = Executor::new(&database.metadata, &database.data_dir)
            .execute_select(&bound)
            .expect("SELECT가 실행되어야 함");
        assert!(rows.is_empty());
        Ok(())
    }
}
