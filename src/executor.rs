use std::{io, path::Path};

use crate::{
    binder::{BoundExpression, BoundSelect},
    page::{Row, RowId},
    parser::ast::Literal,
    schema::{ColumnId, DatabaseMetadata, TableId, TableMetadata},
    table::HeapTable,
    tuple::{TupleError, Value, decode},
};

#[derive(Debug)]
pub enum ExecutorError {
    Io(io::Error),
    TupleError(TupleError),
    TableNotFound(TableId),
    ColumnNotFound(ColumnId),
}

impl From<io::Error> for ExecutorError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<TupleError> for ExecutorError {
    fn from(value: TupleError) -> Self {
        Self::TupleError(value)
    }
}

pub struct Executor<'a> {
    database: &'a DatabaseMetadata,
    table_dir: &'a Path,
}

impl<'a> Executor<'a> {
    pub fn new(database: &'a DatabaseMetadata, table_dir: &'a Path) -> Self {
        Self {
            database,
            table_dir,
        }
    }

    pub fn execute_select(&self, select: &BoundSelect) -> Result<Vec<(RowId, Row)>, ExecutorError> {
        let table_id = select.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let path = self.table_dir.join(format!("{}.tbl", table_id.id()));
        let mut heap_table = HeapTable::open_existing(&path)?;
        let rows = heap_table.scan()?;

        let Some(filter) = select.filter.as_ref() else {
            return Ok(rows);
        };

        Self::filter_rows(rows, table, filter)
    }

    fn filter_rows(
        rows: Vec<(RowId, Row)>,
        table: &TableMetadata,
        filter: &BoundExpression,
    ) -> Result<Vec<(RowId, Row)>, ExecutorError> {
        let BoundExpression::Equal { column_id, value } = filter;
        let literal = value;
        let column_index = table
            .column_index(*column_id)
            .ok_or(ExecutorError::ColumnNotFound(*column_id))?;

        let mut results = Vec::new();
        for (row_id, row) in rows {
            let values = decode(&row, table.columns())?;

            if Self::is_equal(&values[column_index], literal) {
                results.push((row_id, row));
            }
        }

        Ok(results)
    }

    fn is_equal(value: &Value, literal: &Literal) -> bool {
        match (value, literal) {
            (Value::Null, Literal::Null) => false,
            (Value::Int(a), Literal::Integer(b)) => i64::from(*a) == *b,
            (Value::BigInt(a), Literal::Integer(b)) => a == b,
            (Value::Varchar(a), Literal::String(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{create_dir, remove_dir_all},
        io::ErrorKind,
        path::{Path, PathBuf},
        process,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use crate::{
        binder::{BoundExpression, BoundProjection, BoundSelect},
        page::Row,
        parser::ast::Literal,
        schema::{ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata},
        table::HeapTable,
        tuple::{Value, encode},
    };

    use super::{Executor, ExecutorError};

    static NEXT_TEST_DIRECTORY_ID: AtomicUsize = AtomicUsize::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let counter = NEXT_TEST_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
            let path = temp_dir().join(format!(
                "rdb-rs-executor-{label}-{}-{counter}",
                process::id()
            ));
            create_dir(&path).expect("테스트 디렉터리를 생성해야 함");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = remove_dir_all(&self.path);
        }
    }

    fn name_columns() -> Vec<ColumnMetadata> {
        vec![ColumnMetadata::new(
            ColumnId::new(1),
            "name".to_owned(),
            DataType::Varchar,
        )]
    }

    fn database(table_id: TableId) -> DatabaseMetadata {
        let table = TableMetadata::new(table_id, "users".to_owned(), name_columns())
            .expect("테이블 메타데이터가 유효해야 함");
        DatabaseMetadata::new("test".to_owned(), vec![table])
            .expect("데이터베이스 메타데이터가 유효해야 함")
    }

    fn select_all(table_id: TableId) -> BoundSelect {
        BoundSelect {
            table_id,
            projections: vec![BoundProjection::All],
            filter: None,
        }
    }

    fn select_name_equals(table_id: TableId, value: Literal) -> BoundSelect {
        BoundSelect {
            table_id,
            projections: vec![BoundProjection::All],
            filter: Some(BoundExpression::Equal {
                column_id: ColumnId::new(1),
                value,
            }),
        }
    }

    #[test]
    fn select는_테이블의_모든_row를_반환한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let directory = TestDirectory::new("select-all");
        let path = directory.path().join("1.tbl");
        let first_row = Row::from_bytes(&[1, 2]);
        let second_row = Row::from_bytes(&[3, 4]);

        let (first_row_id, second_row_id) = {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            let first_row_id = table.insert(&first_row).expect("첫 Row를 삽입해야 함");
            let second_row_id = table.insert(&second_row).expect("둘째 Row를 삽입해야 함");
            (first_row_id, second_row_id)
        };

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_all(table_id))
            .expect("SELECT가 성공해야 함");

        assert_eq!(
            rows,
            vec![(first_row_id, first_row), (second_row_id, second_row)]
        );
    }

    #[test]
    fn select는_equal_filter와_일치하는_row만_반환한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = name_columns();
        let directory = TestDirectory::new("select-filter");
        let path = directory.path().join("1.tbl");
        let kim =
            encode(&[Value::Varchar("Kim".to_owned())], &columns).expect("Kim Row를 변환해야 함");
        let lee =
            encode(&[Value::Varchar("Lee".to_owned())], &columns).expect("Lee Row를 변환해야 함");

        let kim_row_id = {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            let kim_row_id = table.insert(&kim).expect("Kim Row를 삽입해야 함");
            table.insert(&lee).expect("Lee Row를 삽입해야 함");
            kim_row_id
        };

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_name_equals(
                table_id,
                Literal::String("Kim".to_owned()),
            ))
            .expect("SELECT가 성공해야 함");

        assert_eq!(rows, vec![(kim_row_id, kim)]);
    }

    #[test]
    fn select에서_null_equal_filter는_row를_반환하지_않는다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = name_columns();
        let directory = TestDirectory::new("select-null-filter");
        let path = directory.path().join("1.tbl");
        let null_name = encode(&[Value::Null], &columns).expect("NULL Row를 변환해야 함");

        {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            table.insert(&null_name).expect("NULL Row를 삽입해야 함");
        }

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_name_equals(table_id, Literal::Null))
            .expect("SELECT가 성공해야 함");

        assert!(rows.is_empty());
    }

    #[test]
    fn select는_메타데이터에_없는_테이블을_거부한다() {
        let table_id = TableId::new(1);
        let database = DatabaseMetadata::new("test".to_owned(), vec![])
            .expect("빈 데이터베이스 메타데이터가 유효해야 함");
        let directory = TestDirectory::new("table-not-found");
        let executor = Executor::new(&database, directory.path());

        let result = executor.execute_select(&select_all(table_id));

        assert!(matches!(result, Err(ExecutorError::TableNotFound(id)) if id == table_id));
    }

    #[test]
    fn select는_없는_테이블_파일을_생성하지_않는다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let directory = TestDirectory::new("missing-file");
        let executor = Executor::new(&database, directory.path());

        let result = executor.execute_select(&select_all(table_id));

        assert!(
            matches!(result, Err(ExecutorError::Io(error)) if error.kind() == ErrorKind::NotFound)
        );
        assert!(!directory.path().join("1.tbl").exists());
    }
}
