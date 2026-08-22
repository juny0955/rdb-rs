use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{
    binder::{BoundExpression, BoundInsert, BoundProjection, BoundSelect, BoundUpdate},
    page::{Row, RowId},
    parser::ast::Literal,
    schema::{ColumnId, DataType, DatabaseMetadata, TableId, TableMetadata},
    table::HeapTable,
    tuple::{TupleError, Value, decode, encode},
};

#[derive(Debug)]
pub enum ExecutorError {
    Io(io::Error),
    TupleError(TupleError),
    TableNotFound(TableId),
    ColumnNotFound(ColumnId),
    LiteralTypeMismatch { expected: DataType },
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

    pub fn execute_select(&self, bound: &BoundSelect) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let rows = heap_table.scan()?;

        let projections = &bound.projections;
        let Some(filter) = bound.filter.as_ref() else {
            return Self::project_rows(rows, table, projections);
        };

        let filtered_rows = Self::filter_rows(rows, table, filter)?;
        Self::project_rows(filtered_rows, table, projections)
    }

    pub fn execute_insert(&self, bound: &BoundInsert) -> Result<RowId, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut values = Vec::new();
        for (literal, column) in bound.literals.iter().zip(table.columns()) {
            let value = Self::literal_to_value(literal, column.data_type())?;
            values.push(value);
        }
        let row = encode(&values, table.columns())?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let row_id = heap_table.insert(&row)?;

        Ok(row_id)
    }

    pub fn execute_update(&self, bound: &BoundUpdate) -> Result<usize, ExecutorError> {
        let table_id = bound.table_id;
        let table = self
            .database
            .table_by_id(table_id)
            .ok_or(ExecutorError::TableNotFound(table_id))?;

        let mut heap_table = HeapTable::open_existing(&self.table_path(table_id))?;
        let rows = heap_table.scan()?;

        let rows = {
            if let Some(filter) = bound.filter.as_ref() {
                Self::filter_rows(rows, table, filter)?
            } else {
                rows
            }
        };

        let mut updated = 0;
        for (row_id, row) in rows {
            let mut values = decode(&row, table.columns())?;
            for assignment in &bound.assignments {
                let column_index = table
                    .column_index(assignment.column_id)
                    .ok_or(ExecutorError::ColumnNotFound(assignment.column_id))?;
                let column = &table.columns()[column_index];

                let value = Self::literal_to_value(&assignment.value, column.data_type())?;
                values[column_index] = value;
            }
            let row = encode(&values, table.columns())?;
            heap_table.update(row_id, &row)?;
            updated += 1;
        }

        Ok(updated)
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

    fn project_rows(
        rows: Vec<(RowId, Row)>,
        table: &TableMetadata,
        projections: &[BoundProjection],
    ) -> Result<Vec<Vec<Value>>, ExecutorError> {
        let mut results = Vec::new();

        for (_, row) in rows {
            let values = decode(&row, table.columns())?;
            let mut projection_values = Vec::new();

            for projection in projections {
                match projection {
                    BoundProjection::All => projection_values.extend(values.iter().cloned()),
                    BoundProjection::Column(column_id) => {
                        let column_index = table
                            .column_index(*column_id)
                            .ok_or(ExecutorError::ColumnNotFound(*column_id))?;
                        projection_values.push(values[column_index].clone());
                    }
                }
            }

            results.push(projection_values);
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

    fn literal_to_value(literal: &Literal, data_type: DataType) -> Result<Value, ExecutorError> {
        Ok(match (literal, data_type) {
            (Literal::Null, _) => Value::Null,
            (Literal::Integer(a), DataType::Int) => Value::Int(i32::try_from(*a).map_err(
                |_| ExecutorError::LiteralTypeMismatch {
                    expected: data_type,
                },
            )?),
            (Literal::Integer(a), DataType::BigInt) => Value::BigInt(*a),
            (Literal::String(a), DataType::Varchar) => Value::Varchar(a.to_owned()),
            _ => {
                return Err(ExecutorError::LiteralTypeMismatch {
                    expected: data_type,
                });
            }
        })
    }

    fn table_path(&self, table_id: TableId) -> PathBuf {
        self.table_dir.join(format!("{}.tbl", table_id.id()))
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use crate::{
        binder::{
            BoundAssignment, BoundExpression, BoundInsert, BoundProjection, BoundSelect,
            BoundUpdate,
        },
        parser::ast::Literal,
        schema::{ColumnId, ColumnMetadata, DataType, DatabaseMetadata, TableId, TableMetadata},
        table::HeapTable,
        test_supports::TestDirectory,
        tuple::{Value, decode, encode},
    };

    use super::{Executor, ExecutorError};

    fn users_columns() -> Vec<ColumnMetadata> {
        vec![
            ColumnMetadata::new(ColumnId::new(1), "id".to_owned(), DataType::BigInt),
            ColumnMetadata::new(ColumnId::new(2), "name".to_owned(), DataType::Varchar),
        ]
    }

    fn database(table_id: TableId) -> DatabaseMetadata {
        let table = TableMetadata::new(table_id, "users".to_owned(), users_columns())
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

    fn select_name(table_id: TableId) -> BoundSelect {
        BoundSelect {
            table_id,
            projections: vec![BoundProjection::Column(ColumnId::new(2))],
            filter: None,
        }
    }

    fn select_name_then_all(table_id: TableId) -> BoundSelect {
        BoundSelect {
            table_id,
            projections: vec![
                BoundProjection::Column(ColumnId::new(2)),
                BoundProjection::All,
            ],
            filter: None,
        }
    }

    fn select_name_equals(table_id: TableId, value: Literal) -> BoundSelect {
        BoundSelect {
            table_id,
            projections: vec![BoundProjection::All],
            filter: Some(BoundExpression::Equal {
                column_id: ColumnId::new(2),
                value,
            }),
        }
    }

    #[test]
    fn insert는_리터럴을_row로_변환해_테이블에_저장한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("insert");
        let path = directory.path().join("1.tbl");
        let bound = BoundInsert {
            table_id,
            literals: vec![Literal::Integer(1), Literal::String("Kim".to_owned())],
        };

        HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
        let executor = Executor::new(&database, directory.path());

        let row_id = executor
            .execute_insert(&bound)
            .expect("INSERT가 성공해야 함");

        let mut table = HeapTable::open_existing(&path).expect("테이블 파일을 열어야 함");
        let row = table.get(row_id).expect("삽입한 Row를 읽어야 함");
        let values = decode(&row, &columns).expect("Row를 값으로 변환해야 함");

        assert_eq!(
            values,
            vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())]
        );
    }

    #[test]
    fn update는_필터와_일치하는_row만_수정하고_재시작후에도_유지한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("update-filter");
        let path = directory.path().join("1.tbl");
        let kim = encode(
            &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
            &columns,
        )
        .expect("Kim Row를 변환해야 함");
        let lee = encode(
            &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
            &columns,
        )
        .expect("Lee Row를 변환해야 함");

        let (kim_id, lee_id) = {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            let kim_id = table.insert(&kim).expect("Kim Row를 삽입해야 함");
            let lee_id = table.insert(&lee).expect("Lee Row를 삽입해야 함");
            (kim_id, lee_id)
        };

        let bound = BoundUpdate {
            table_id,
            assignments: vec![BoundAssignment {
                column_id: ColumnId::new(2),
                value: Literal::String("Park".to_owned()),
            }],
            filter: Some(BoundExpression::Equal {
                column_id: ColumnId::new(1),
                value: Literal::Integer(1),
            }),
        };

        let executor = Executor::new(&database, directory.path());
        let updated = executor
            .execute_update(&bound)
            .expect("UPDATE가 성공해야 함");

        assert_eq!(updated, 1);

        let mut table = HeapTable::open_existing(&path).expect("테이블 파일을 다시 열어야 함");
        let kim = decode(
            &table.get(kim_id).expect("수정한 Kim Row를 읽어야 함"),
            &columns,
        )
        .expect("Kim Row를 값으로 변환해야 함");
        let lee = decode(
            &table.get(lee_id).expect("유지된 Lee Row를 읽어야 함"),
            &columns,
        )
        .expect("Lee Row를 값으로 변환해야 함");

        assert_eq!(
            kim,
            vec![Value::BigInt(1), Value::Varchar("Park".to_owned())]
        );
        assert_eq!(
            lee,
            vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())]
        );
    }

    #[test]
    fn select는_테이블의_모든_row를_반환한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("select-all");
        let path = directory.path().join("1.tbl");
        let first_values = vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())];
        let second_values = vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())];
        let first_row = encode(&first_values, &columns).expect("첫 Row를 변환해야 함");
        let second_row = encode(&second_values, &columns).expect("둘째 Row를 변환해야 함");

        {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            table.insert(&first_row).expect("첫 Row를 삽입해야 함");
            table.insert(&second_row).expect("둘째 Row를 삽입해야 함");
        }

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_all(table_id))
            .expect("SELECT가 성공해야 함");

        assert_eq!(rows, vec![first_values, second_values]);
    }

    #[test]
    fn select는_지정한_컬럼만_반환한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("select-name");
        let path = directory.path().join("1.tbl");
        let kim = encode(
            &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
            &columns,
        )
        .expect("Kim Row를 변환해야 함");
        let lee = encode(
            &[Value::BigInt(2), Value::Varchar("Lee".to_owned())],
            &columns,
        )
        .expect("Lee Row를 변환해야 함");

        {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            table.insert(&kim).expect("Kim Row를 삽입해야 함");
            table.insert(&lee).expect("Lee Row를 삽입해야 함");
        }

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_name(table_id))
            .expect("SELECT가 성공해야 함");

        assert_eq!(
            rows,
            vec![
                vec![Value::Varchar("Kim".to_owned())],
                vec![Value::Varchar("Lee".to_owned())],
            ]
        );
    }

    #[test]
    fn select는_projection_목록_순서대로_값을_반환한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("select-name-then-all");
        let path = directory.path().join("1.tbl");
        let row = encode(
            &[Value::BigInt(1), Value::Varchar("Kim".to_owned())],
            &columns,
        )
        .expect("Row를 변환해야 함");

        {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            table.insert(&row).expect("Row를 삽입해야 함");
        }

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_name_then_all(table_id))
            .expect("SELECT가 성공해야 함");

        assert_eq!(
            rows,
            vec![vec![
                Value::Varchar("Kim".to_owned()),
                Value::BigInt(1),
                Value::Varchar("Kim".to_owned()),
            ]]
        );
    }

    #[test]
    fn select는_equal_filter와_일치하는_row만_반환한다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("select-filter");
        let path = directory.path().join("1.tbl");
        let kim_values = vec![Value::BigInt(1), Value::Varchar("Kim".to_owned())];
        let lee_values = vec![Value::BigInt(2), Value::Varchar("Lee".to_owned())];
        let kim = encode(&kim_values, &columns).expect("Kim Row를 변환해야 함");
        let lee = encode(&lee_values, &columns).expect("Lee Row를 변환해야 함");

        {
            let mut table = HeapTable::open(&path).expect("테이블 파일을 생성해야 함");
            table.insert(&kim).expect("Kim Row를 삽입해야 함");
            table.insert(&lee).expect("Lee Row를 삽입해야 함");
        }

        let executor = Executor::new(&database, directory.path());
        let rows = executor
            .execute_select(&select_name_equals(
                table_id,
                Literal::String("Kim".to_owned()),
            ))
            .expect("SELECT가 성공해야 함");

        assert_eq!(rows, vec![kim_values]);
    }

    #[test]
    fn select에서_null_equal_filter는_row를_반환하지_않는다() {
        let table_id = TableId::new(1);
        let database = database(table_id);
        let columns = users_columns();
        let directory = TestDirectory::new("select-null-filter");
        let path = directory.path().join("1.tbl");
        let null_name =
            encode(&[Value::BigInt(1), Value::Null], &columns).expect("NULL Row를 변환해야 함");

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
