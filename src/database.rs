use std::{
    collections::{HashMap, hash_map::Entry},
    io,
    path::{Path, PathBuf},
};

use crate::{
    binder::{Binder, BinderError, BoundCreateTable, BoundStatement},
    buffer::BufferPool,
    catalog::{Catalog, CatalogError},
    executor::{Executor, ExecutorError},
    parser::ast::{Literal, Statement},
    schema::{ColumnId, ColumnMetadata, DatabaseMetadata, SchemaError, TableId, TableMetadata},
    table::{HeapTable, HeapTableError},
    tuple::Value,
};
use crate::{
    binder::{BoundCreateIndex, BoundExpression},
    file::open_rw_create,
    index::{
        BTreeKey, BTreeKeyType,
        tree::{BTree, BTreeError},
    },
    page::RowId,
    schema::{IndexId, IndexMetadata, RelationId},
    tuple::{self, TupleError},
};
use thiserror::Error;

const BUFFER_POOL_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error(transparent)]
    Binder(#[from] BinderError),
    #[error(transparent)]
    Executor(#[from] ExecutorError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error(transparent)]
    HeapTable(#[from] HeapTableError),
    #[error(transparent)]
    BTree(#[from] BTreeError),
    #[error(transparent)]
    Tuple(#[from] TupleError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("인덱스 엔트리를 찾을 수 없습니다: index={index_id:?}, row={row_id:?}")]
    IndexEntryNotFound { index_id: IndexId, row_id: RowId },
    #[error("INT 인덱스 키가 i32 범위를 벗어났습니다: {value}")]
    IndexKeyOutOfRange { value: i64 },
    #[error("literal과 인덱스 키 타입이 일치하지 않습니다: {key_type:?}")]
    IndexKeyTypeMismatch { key_type: BTreeKeyType },
}

#[derive(Debug)]
pub enum ExecuteResult {
    Success,
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
                let metadata = DatabaseMetadata::new(name.to_owned(), vec![], vec![])?;
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
                Ok(ExecuteResult::Success)
            }
            BoundStatement::CreateIndex(b) => {
                self.create_index(&b)?;
                Ok(ExecuteResult::Success)
            }
            BoundStatement::Insert(b) => {
                let heap_table = Self::get_or_open_table(
                    &mut self.heap_tables,
                    &mut self.buffer_pool,
                    &self.data_dir,
                    b.table_id,
                )?;
                let executor = Executor::new(&self.metadata);
                let result = executor.execute_insert(&b, heap_table, &mut self.buffer_pool)?;
                self.insert_row_into_indexes(b.table_id, result.row_id, result.values)?;
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

                if let Some(filter) = &b.filter {
                    match filter {
                        BoundExpression::Equal { column_id, value } => {
                            if let Some(index_metadata) =
                                self.metadata.indexes().iter().find(|index| {
                                    index.table_id() == b.table_id
                                        && index.column_id() == *column_id
                                })
                            {
                                let table = self
                                    .metadata
                                    .table_by_id(index_metadata.table_id())
                                    .ok_or(DatabaseError::Schema(
                                        SchemaError::IndexTableNotFound(index_metadata.table_id()),
                                    ))?;
                                let column = table.column_by_id(index_metadata.column_id()).ok_or(
                                    DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                                        table_id: index_metadata.table_id(),
                                        column_id: index_metadata.column_id(),
                                    }),
                                )?;
                                let key_type = BTreeKeyType::try_from(column.data_type())?;
                                self.buffer_pool.register_relation(
                                    RelationId::Index(index_metadata.id()),
                                    self.data_dir
                                        .join(format!("{}.idx", index_metadata.id().id())),
                                );
                                let btree = BTree::open(
                                    index_metadata.id(),
                                    index_metadata.root_page_id(),
                                    key_type,
                                );

                                let rows = if let Some(key) = literal_to_btree_key(value, key_type)?
                                {
                                    let row_ids = btree.search_all(&mut self.buffer_pool, key)?;

                                    let mut rows = Vec::new();
                                    for row_id in row_ids {
                                        let row = heap_table.get(row_id, &mut self.buffer_pool)?;
                                        rows.push((row_id, row));
                                    }
                                    rows
                                } else {
                                    vec![]
                                };

                                let results = executor.projection_and_filtered_rows(rows, &b)?;
                                return Ok(ExecuteResult::Rows(results));
                            }
                        }
                    }
                }

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
                let results = executor.execute_update(&b, heap_table, &mut self.buffer_pool)?;
                let affected_rows = results.len();

                for result in results {
                    self.delete_row_from_indexes(b.table_id, result.row_id, result.old_values)?;
                    self.insert_row_into_indexes(b.table_id, result.row_id, result.new_values)?;
                }
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
                let results = executor.execute_delete(&b, heap_table, &mut self.buffer_pool)?;
                let affected_rows = results.len();

                for result in results {
                    self.delete_row_from_indexes(b.table_id, result.row_id, result.values)?;
                }

                Ok(ExecuteResult::Command { affected_rows })
            }
        }
    }

    fn create_index(&mut self, bound: &BoundCreateIndex) -> Result<(), DatabaseError> {
        if self.metadata.index(&bound.index_name).is_some() {
            return Err(DatabaseError::Schema(SchemaError::DuplicateIndexName(
                bound.index_name.to_owned(),
            )));
        }

        if self.metadata.indexes().len() >= usize::from(u16::MAX) {
            return Err(DatabaseError::Schema(SchemaError::TooManyIndexes));
        }

        let index_id = IndexId::new(self.metadata.indexes().len() as u32 + 1);
        let table = self
            .metadata
            .table_by_id(bound.table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                bound.table_id,
            )))?;
        let column = table
            .column_by_id(bound.column_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                table_id: bound.table_id,
                column_id: bound.column_id,
            }))?;

        let key_type = BTreeKeyType::try_from(column.data_type())?;

        let index_path = self.data_dir.join(format!("{}.idx", index_id.id()));
        open_rw_create(&index_path)?;
        self.buffer_pool
            .register_relation(RelationId::Index(index_id), index_path);

        let btree = BTree::create(index_id, key_type, &mut self.buffer_pool)?;
        let index_metadata = IndexMetadata::new(
            index_id,
            bound.index_name.clone(),
            table.id(),
            column.id(),
            btree.root_page_id(),
        );

        let heap_table = Self::get_or_open_table(
            &mut self.heap_tables,
            &mut self.buffer_pool,
            &self.data_dir,
            table.id(),
        )?;
        let rows = heap_table.scan(&mut self.buffer_pool)?;

        let index = table
            .column_index(column.id())
            .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                table_id: table.id(),
                column_id: column.id(),
            }))?;
        for (row_id, row) in rows {
            let values = tuple::decode(&row, table.columns())?;
            if let Some(key) = Self::value_to_btree_key(values[index].clone()) {
                btree.insert(&mut self.buffer_pool, key, row_id)?;
            }
        }

        self.metadata.add_index(index_metadata)?;
        self.catalog.save(&self.metadata)?;

        Ok(())
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
        self.buffer_pool.register_relation(
            RelationId::Heap(table_id),
            self.data_dir.join(format!("{}.tbl", table_id.id())),
        );

        Ok(table_id)
    }

    fn insert_row_into_indexes(
        &mut self,
        table_id: TableId,
        row_id: RowId,
        values: Vec<Value>,
    ) -> Result<(), DatabaseError> {
        let indexes = self
            .metadata
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table_id)
            .collect::<Vec<_>>();

        let table = self
            .metadata
            .table_by_id(table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            )))?;

        for index in indexes {
            let column = table
                .column_by_id(index.column_id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                    table_id,
                    column_id: index.column_id(),
                }))?;
            let column_index = table
                .column_index(column.id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                    table_id,
                    column_id: index.column_id(),
                }))?;
            if let Some(key) = Self::value_to_btree_key(values[column_index].clone()) {
                let index_id = index.id();
                self.buffer_pool.register_relation(
                    RelationId::Index(index_id),
                    self.data_dir.join(format!("{}.idx", index_id.id())),
                );
                let btree = BTree::open(index_id, index.root_page_id(), key.key_type());
                btree.insert(&mut self.buffer_pool, key, row_id)?;
            } else {
                continue;
            }
        }

        Ok(())
    }

    fn delete_row_from_indexes(
        &mut self,
        table_id: TableId,
        row_id: RowId,
        values: Vec<Value>,
    ) -> Result<(), DatabaseError> {
        let indexes = self
            .metadata
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table_id)
            .collect::<Vec<_>>();

        let table = self
            .metadata
            .table_by_id(table_id)
            .ok_or(DatabaseError::Schema(SchemaError::IndexTableNotFound(
                table_id,
            )))?;

        for index in indexes {
            let column = table
                .column_by_id(index.column_id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                    table_id,
                    column_id: index.column_id(),
                }))?;
            let column_index = table
                .column_index(column.id())
                .ok_or(DatabaseError::Schema(SchemaError::IndexColumnNotFound {
                    table_id,
                    column_id: index.column_id(),
                }))?;
            if let Some(key) = Self::value_to_btree_key(values[column_index].clone()) {
                let index_id = index.id();
                self.buffer_pool.register_relation(
                    RelationId::Index(index_id),
                    self.data_dir.join(format!("{}.idx", index_id.id())),
                );
                let btree = BTree::open(index_id, index.root_page_id(), key.key_type());
                if !btree.delete(&mut self.buffer_pool, key, row_id)? {
                    return Err(DatabaseError::IndexEntryNotFound { index_id, row_id });
                }
            } else {
                continue;
            }
        }

        Ok(())
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
                buffer_pool.register_relation(
                    RelationId::Heap(table_id),
                    data_dir.join(format!("{}.tbl", table_id.id())),
                );
                Ok(entry.insert(table))
            }
        }
    }

    fn value_to_btree_key(value: Value) -> Option<BTreeKey> {
        match value {
            Value::Int(v) => Some(BTreeKey::Int(v)),
            Value::BigInt(v) => Some(BTreeKey::BigInt(v)),
            Value::Boolean(v) => Some(BTreeKey::Boolean(v)),
            Value::Varchar(v) => Some(BTreeKey::Varchar(v)),
            Value::Null => None,
        }
    }
}

fn literal_to_btree_key(
    literal: &Literal,
    key_type: BTreeKeyType,
) -> Result<Option<BTreeKey>, DatabaseError> {
    match (literal, key_type) {
        (Literal::Integer(v), BTreeKeyType::Int) => {
            Ok(Some(BTreeKey::Int(i32::try_from(*v).map_err(|_| {
                DatabaseError::IndexKeyOutOfRange { value: *v }
            })?)))
        }
        (Literal::Integer(v), BTreeKeyType::BigInt) => Ok(Some(BTreeKey::BigInt(*v))),
        (Literal::String(v), BTreeKeyType::Varchar) => Ok(Some(BTreeKey::Varchar(v.clone()))),
        (Literal::Null, _) => Ok(None),
        _ => Err(DatabaseError::IndexKeyTypeMismatch { key_type }),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        binder::{Binder, BoundCreateTable, BoundStatement},
        executor::Executor,
        page::PageId,
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
    fn create_index는_index_file과_metadata를_생성하고_재시작후에도_유지한다()
    -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-create-index");
        {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;

            assert!(directory.path().join("1.idx").exists());
        }

        let database = Database::open(directory.path(), "reopened")?;
        let index = database
            .metadata
            .index("idx_users_id")
            .expect("재시작 후 index metadata가 있어야 함");

        assert_eq!(index.root_page_id(), PageId::new(0));
        Ok(())
    }

    #[test]
    fn 재시작후_index_scan은_중복_key의_모든_row를_반환한다() -> Result<(), DatabaseError> {
        // Given
        let directory = TestDirectory::new("database-index-scan-duplicates");
        {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Lee');"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (7, 'Park');"))?;
        }
        let mut database = Database::open(directory.path(), "reopened")?;

        // When
        let result = database.execute(&parse_sql("SELECT name FROM users WHERE id = 42;"))?;

        // Then
        assert!(matches!(
            result,
            ExecuteResult::Rows(rows)
                if rows == vec![
                    vec![Value::Varchar("Kim".to_owned())],
                    vec![Value::Varchar("Lee".to_owned())],
                ]
        ));
        Ok(())
    }

    #[test]
    fn create_index는_재시작후_기존_행을_backfill한다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-create-index-backfill");
        let indexed_row_id = {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (NULL, 'Null');"))?;

            let heap_table = Database::get_or_open_table(
                &mut database.heap_tables,
                &mut database.buffer_pool,
                database.data_dir.as_path(),
                TableId::new(1),
            )?;
            heap_table
                .scan(&mut database.buffer_pool)?
                .into_iter()
                .next()
                .expect("인덱싱할 행이 있어야 함")
                .0
        };

        {
            let mut database = Database::open(directory.path(), "reopened")?;
            database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
        }

        let mut database = Database::open(directory.path(), "reopened-again")?;
        let index = database
            .metadata
            .index("idx_users_id")
            .expect("index metadata가 있어야 함");
        database.buffer_pool.register_relation(
            RelationId::Index(index.id()),
            directory.path().join(format!("{}.idx", index.id().id())),
        );
        let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

        assert_eq!(
            btree.search(&mut database.buffer_pool, BTreeKey::BigInt(42))?,
            Some(indexed_row_id)
        );
        Ok(())
    }

    #[test]
    fn insert는_생성된_index에_반영하고_재시작후_검색된다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-insert-index-maintenance");
        let inserted_row_id = {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;

            let heap_table = Database::get_or_open_table(
                &mut database.heap_tables,
                &mut database.buffer_pool,
                database.data_dir.as_path(),
                TableId::new(1),
            )?;
            heap_table
                .scan(&mut database.buffer_pool)?
                .into_iter()
                .next()
                .expect("삽입한 행이 있어야 함")
                .0
        };

        let mut database = Database::open(directory.path(), "reopened")?;
        let index = database
            .metadata
            .index("idx_users_id")
            .expect("index metadata가 있어야 함");
        database.buffer_pool.register_relation(
            RelationId::Index(index.id()),
            directory.path().join(format!("{}.idx", index.id().id())),
        );
        let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

        assert_eq!(
            btree.search(&mut database.buffer_pool, BTreeKey::BigInt(42))?,
            Some(inserted_row_id)
        );
        Ok(())
    }

    #[test]
    fn delete는_index_entry를_제거하고_재시작후_검색되지_않는다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-delete-index-maintenance");
        {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;
            database.execute(&parse_sql("DELETE FROM users WHERE id = 42;"))?;
        }

        let mut database = Database::open(directory.path(), "reopened")?;
        let index = database
            .metadata
            .index("idx_users_id")
            .expect("index metadata가 있어야 함");
        database.buffer_pool.register_relation(
            RelationId::Index(index.id()),
            directory.path().join(format!("{}.idx", index.id().id())),
        );
        let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

        assert_eq!(
            btree.search(&mut database.buffer_pool, BTreeKey::BigInt(42))?,
            None
        );
        Ok(())
    }

    #[test]
    fn update는_index_entry를_교체하고_재시작후_검색된다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-update-index-maintenance");
        let row_id = {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("CREATE INDEX idx_users_id ON users(id);"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (42, 'Kim');"))?;

            let heap_table = Database::get_or_open_table(
                &mut database.heap_tables,
                &mut database.buffer_pool,
                database.data_dir.as_path(),
                TableId::new(1),
            )?;
            let row_id = heap_table
                .scan(&mut database.buffer_pool)?
                .into_iter()
                .next()
                .expect("수정할 행이 있어야 함")
                .0;

            database.execute(&parse_sql("UPDATE users SET id = 100 WHERE id = 42;"))?;
            row_id
        };

        let mut database = Database::open(directory.path(), "reopened")?;
        let index = database
            .metadata
            .index("idx_users_id")
            .expect("index metadata가 있어야 함");
        database.buffer_pool.register_relation(
            RelationId::Index(index.id()),
            directory.path().join(format!("{}.idx", index.id().id())),
        );
        let btree = BTree::open(index.id(), index.root_page_id(), BTreeKeyType::BigInt);

        assert_eq!(
            btree.search(&mut database.buffer_pool, BTreeKey::BigInt(42))?,
            None
        );
        assert_eq!(
            btree.search(&mut database.buffer_pool, BTreeKey::BigInt(100))?,
            Some(row_id)
        );
        Ok(())
    }

    #[test]
    fn value_to_btree_key는_null을_건너뛴다() {
        assert_eq!(Database::value_to_btree_key(Value::Null), None);
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

    fn parse_sql(sql: &str) -> Statement {
        let tokens = Lexer::new(sql).tokenize().expect("SQL을 토큰화해야 함");
        Parser::new(tokens).parse().expect("SQL을 파싱해야 함")
    }

    #[test]
    fn 두번째_select는_cache된_page를_사용한다() -> Result<(), DatabaseError> {
        let directory = TestDirectory::new("database-select-cache-hit");
        {
            let mut database = Database::open(directory.path(), "test")?;
            database.execute(&parse_sql("CREATE TABLE users (id BIGINT, name VARCHAR);"))?;
            database.execute(&parse_sql("INSERT INTO users VALUES (1, 'Kim');"))?;
        }

        let mut database = Database::open(directory.path(), "reopened")?;
        let select = parse_sql("SELECT * FROM users;");
        let expected = vec![vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())]];

        assert!(matches!(
            database.execute(&select)?,
            ExecuteResult::Rows(rows) if rows == expected
        ));

        std::fs::rename(
            directory.path().join("1.tbl"),
            directory.path().join("cached-1.tbl"),
        )
        .expect("첫 SELECT 후 table file 이름을 변경해야 함");

        assert!(matches!(
            database.execute(&select)?,
            ExecuteResult::Rows(rows) if rows == expected
        ));
        Ok(())
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
        assert_eq!(updated.len(), 1);

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
        assert_eq!(deleted.len(), 1);

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
