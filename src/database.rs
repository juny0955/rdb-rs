use std::{
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
};

use crate::{
    binder::{Binder, BinderError, BoundCreateTable, BoundStatement},
    buffer::BufferPool,
    catalog::{Catalog, CatalogError},
    executor::{Executor, ExecutorError},
    parser::ast::Statement,
    schema::{ColumnId, ColumnMetadata, DatabaseMetadata, SchemaError, TableId, TableMetadata},
    table::{HeapTable, HeapTableError},
    tuple::Value,
};
use thiserror::Error;

const BUFFER_POOL_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("binder 오류: {0}")]
    Binder(#[from] BinderError),
    #[error("executor 오류: {0}")]
    Executor(#[from] ExecutorError),
    #[error("database catalog 오류: {0}")]
    Catalog(#[from] CatalogError),
    #[error("database schema 오류: {0}")]
    Schema(#[from] SchemaError),
    #[error("heap table 처리 오류: {0}")]
    HeapTable(#[from] HeapTableError),
}

#[derive(Debug)]
pub enum ExecuteResult {
    Command { affected_rows: usize },
    Rows(Vec<Vec<Value>>),
}

#[derive(Debug)]
pub struct Database {
    metadata: DatabaseMetadata,
    catalog: Catalog,
    buffer_pool: BufferPool,
    heap_tables: HashMap<TableId, HeapTable>,
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
            buffer_pool: BufferPool::new(BUFFER_POOL_CAPACITY),
            heap_tables: HashMap::new(),
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn execute(&mut self, statement: &Statement) -> Result<ExecuteResult, DatabaseError> {
        let bound = Binder::new(&self.metadata).bind(statement)?;
        match bound {
            BoundStatement::CreateTable(b) => {
                let _ = self.create_table(&b)?;
                Ok(ExecuteResult::Command { affected_rows: 0 })
            }
            BoundStatement::Insert(b) => {
                let heap_table = Self::get_or_open_table(
                    &mut self.heap_tables,
                    &mut self.buffer_pool,
                    &self.data_dir,
                    b.table_id,
                )?;
                let executor = Executor::new(&self.metadata);
                let _ = executor.execute_insert(&b, heap_table, &mut self.buffer_pool)?;
                Ok(ExecuteResult::Command { affected_rows: 1 })
            }
            BoundStatement::Select(b) => {
                let heap_table = Self::get_or_open_table(
                    &mut self.heap_tables,
                    &mut self.buffer_pool,
                    &self.data_dir,
                    b.table_id,
                )?;
                let executor = Executor::new(&self.metadata);
                let results = executor.execute_select(&b, heap_table, &mut self.buffer_pool)?;
                Ok(ExecuteResult::Rows(results))
            }
            BoundStatement::Update(b) => {
                let heap_table = Self::get_or_open_table(
                    &mut self.heap_tables,
                    &mut self.buffer_pool,
                    &self.data_dir,
                    b.table_id,
                )?;
                let executor = Executor::new(&self.metadata);
                let affected_rows =
                    executor.execute_update(&b, heap_table, &mut self.buffer_pool)?;
                Ok(ExecuteResult::Command { affected_rows })
            }
            BoundStatement::Delete(b) => {
                let heap_table = Self::get_or_open_table(
                    &mut self.heap_tables,
                    &mut self.buffer_pool,
                    &self.data_dir,
                    b.table_id,
                )?;
                let executor = Executor::new(&self.metadata);
                let affected_rows =
                    executor.execute_delete(&b, heap_table, &mut self.buffer_pool)?;
                Ok(ExecuteResult::Command { affected_rows })
            }
        }
    }

    fn create_table(&mut self, bound: &BoundCreateTable) -> Result<TableId, DatabaseError> {
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

        let heap_table = HeapTable::open(
            table_id,
            &self.data_dir.join(format!("{}.tbl", table_id.id())),
        )?;
        self.heap_tables.insert(table_id, heap_table);
        self.metadata.add_table(table)?;
        self.catalog.save(&self.metadata)?;
        self.buffer_pool.register_table(
            table_id,
            self.data_dir.join(format!("{}.tbl", table_id.id())),
        );

        Ok(table_id)
    }

    fn get_or_open_table<'a>(
        heap_tables: &'a mut HashMap<TableId, HeapTable>,
        buffer_pool: &mut BufferPool,
        data_dir: &Path,
        table_id: TableId,
    ) -> Result<&'a mut HeapTable, DatabaseError> {
        match heap_tables.entry(table_id) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let table = HeapTable::open_existing(
                    table_id,
                    &data_dir.join(format!("{}.tbl", table_id.id())),
                )?;
                buffer_pool
                    .register_table(table_id, data_dir.join(format!("{}.tbl", table_id.id())));
                Ok(entry.insert(table))
            }
        }
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

    #[test]
    fn get_or_open_table은_같은_table을_한번만_등록한다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-table-registry");
        let table_id = {
            let mut database = Database::open(directory.path(), "test")?;
            database.create_table(&users_table())?
        };

        let mut database = Database::open(directory.path(), "reopened")?;
        {
            let _ = Database::get_or_open_table(
                &mut database.heap_tables,
                &mut database.buffer_pool,
                database.data_dir.as_path(),
                table_id,
            )?;
        }
        assert_eq!(database.heap_tables.len(), 1);

        {
            let _ = Database::get_or_open_table(
                &mut database.heap_tables,
                &mut database.buffer_pool,
                database.data_dir.as_path(),
                table_id,
            )?;
        }
        assert_eq!(database.heap_tables.len(), 1);

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
        let heap_table = Database::get_or_open_table(
            &mut database.heap_tables,
            &mut database.buffer_pool,
            database.data_dir.as_path(),
            bound.table_id,
        )?;
        Executor::new(&database.metadata)
            .execute_insert(&bound, heap_table, &mut database.buffer_pool)
            .expect("INSERT가 실행되어야 함");

        let BoundStatement::Select(bound) =
            bind_sql("SELECT name FROM users WHERE id = 1;", &database.metadata)
        else {
            panic!("SELECT가 bind되어야 함");
        };
        let heap_table = Database::get_or_open_table(
            &mut database.heap_tables,
            &mut database.buffer_pool,
            database.data_dir.as_path(),
            bound.table_id,
        )?;
        let rows = Executor::new(&database.metadata)
            .execute_select(&bound, heap_table, &mut database.buffer_pool)
            .expect("SELECT가 실행되어야 함");
        assert_eq!(rows, vec![vec![Value::Varchar("Kim".to_owned())]]);

        let BoundStatement::Update(bound) = bind_sql(
            "UPDATE users SET name = 'Lee' WHERE id = 1;",
            &database.metadata,
        ) else {
            panic!("UPDATE가 bind되어야 함");
        };
        let heap_table = Database::get_or_open_table(
            &mut database.heap_tables,
            &mut database.buffer_pool,
            database.data_dir.as_path(),
            bound.table_id,
        )?;
        let updated = Executor::new(&database.metadata)
            .execute_update(&bound, heap_table, &mut database.buffer_pool)
            .expect("UPDATE가 실행되어야 함");
        assert_eq!(updated, 1);

        let BoundStatement::Select(bound) = bind_sql("SELECT * FROM users;", &database.metadata)
        else {
            panic!("SELECT가 bind되어야 함");
        };
        let heap_table = Database::get_or_open_table(
            &mut database.heap_tables,
            &mut database.buffer_pool,
            database.data_dir.as_path(),
            bound.table_id,
        )?;
        let rows = Executor::new(&database.metadata)
            .execute_select(&bound, heap_table, &mut database.buffer_pool)
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
        let heap_table = Database::get_or_open_table(
            &mut database.heap_tables,
            &mut database.buffer_pool,
            database.data_dir.as_path(),
            bound.table_id,
        )?;
        let deleted = Executor::new(&database.metadata)
            .execute_delete(&bound, heap_table, &mut database.buffer_pool)
            .expect("DELETE가 실행되어야 함");
        assert_eq!(deleted, 1);

        let BoundStatement::Select(bound) = bind_sql("SELECT * FROM users;", &database.metadata)
        else {
            panic!("SELECT가 bind되어야 함");
        };
        let heap_table = Database::get_or_open_table(
            &mut database.heap_tables,
            &mut database.buffer_pool,
            database.data_dir.as_path(),
            bound.table_id,
        )?;
        let rows = Executor::new(&database.metadata)
            .execute_select(&bound, heap_table, &mut database.buffer_pool)
            .expect("SELECT가 실행되어야 함");
        assert!(rows.is_empty());
        Ok(())
    }
}
