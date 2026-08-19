use std::str::from_utf8;

use crate::{
    page::Row,
    schema::{ColumnMetadata, DataType},
};

const NULL_MARKER: u8 = 0;
const NOT_NULL_MARKER: u8 = 1;

const BOOLEAN_FALSE: u8 = 0;
const BOOLEAN_TRUE: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TupleError {
    ValueCountMismatch,
    TypeMismatch,
    VarcharTooLong,
    InvalidNullMarker,
    InvalidBoolean,
    InvalidUtf8,
    TruncatedRow,
    TrailingBytes,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    BigInt(i64),
    Boolean(bool),
    Varchar(String),
    Null,
}

pub fn encode(values: &[Value], columns: &[ColumnMetadata]) -> Result<Row, TupleError> {
    if values.len() != columns.len() {
        return Err(TupleError::ValueCountMismatch);
    }

    let mut bytes: Vec<u8> = Vec::new();
    for (value, column) in values.iter().zip(columns) {
        if value == &Value::Null {
            bytes.push(NULL_MARKER);
            continue;
        }

        match (value, column.data_type()) {
            (Value::Int(v), DataType::Int) => {
                bytes.push(NOT_NULL_MARKER);
                bytes.extend_from_slice(&v.to_be_bytes());
            }
            (Value::BigInt(v), DataType::BigInt) => {
                bytes.push(NOT_NULL_MARKER);
                bytes.extend_from_slice(&v.to_be_bytes());
            }
            (Value::Boolean(v), DataType::Boolean) => {
                bytes.push(NOT_NULL_MARKER);
                if *v {
                    bytes.push(BOOLEAN_TRUE);
                } else {
                    bytes.push(BOOLEAN_FALSE);
                }
            }
            (Value::Varchar(v), DataType::Varchar) => {
                let string_bytes = v.as_bytes();
                let length =
                    u16::try_from(string_bytes.len()).map_err(|_| TupleError::VarcharTooLong)?;

                bytes.push(NOT_NULL_MARKER);
                bytes.extend_from_slice(&length.to_be_bytes());
                bytes.extend_from_slice(string_bytes);
            }
            _ => return Err(TupleError::TypeMismatch),
        }
    }

    Ok(Row::from_bytes(&bytes))
}

pub fn decode(row: &Row, columns: &[ColumnMetadata]) -> Result<Vec<Value>, TupleError> {
    let bytes = row.to_bytes();
    let mut cursor = 0;
    let mut values = Vec::new();

    for column in columns {
        let marker = take(bytes, &mut cursor, 1)?[0];
        if marker == NULL_MARKER {
            values.push(Value::Null);
            continue;
        }

        if marker != NOT_NULL_MARKER {
            return Err(TupleError::InvalidNullMarker);
        }

        match column.data_type() {
            DataType::Int => {
                let slice = take(bytes, &mut cursor, 4)?;
                let value_bytes: [u8; 4] =
                    slice.try_into().map_err(|_| TupleError::TruncatedRow)?;
                values.push(Value::Int(i32::from_be_bytes(value_bytes)));
            }
            DataType::BigInt => {
                let slice = take(bytes, &mut cursor, 8)?;
                let value_bytes: [u8; 8] =
                    slice.try_into().map_err(|_| TupleError::TruncatedRow)?;
                values.push(Value::BigInt(i64::from_be_bytes(value_bytes)));
            }
            DataType::Boolean => {
                let bool_byte = take(bytes, &mut cursor, 1)?[0];
                if bool_byte == BOOLEAN_TRUE {
                    values.push(Value::Boolean(true));
                } else if bool_byte == BOOLEAN_FALSE {
                    values.push(Value::Boolean(false));
                } else {
                    return Err(TupleError::InvalidBoolean);
                }
            }
            DataType::Varchar => {
                let length_bytes: [u8; 2] = take(bytes, &mut cursor, 2)?
                    .try_into()
                    .map_err(|_| TupleError::TruncatedRow)?;
                let length = usize::from(u16::from_be_bytes(length_bytes));

                let string_bytes = take(bytes, &mut cursor, length)?;
                let value = from_utf8(string_bytes)
                    .map_err(|_| TupleError::InvalidUtf8)?
                    .to_owned();

                values.push(Value::Varchar(value));
            }
            DataType::Null => return Err(TupleError::TypeMismatch),
        }
    }

    if cursor != bytes.len() {
        return Err(TupleError::TrailingBytes);
    }

    Ok(values)
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8], TupleError> {
    let end = cursor.checked_add(length).ok_or(TupleError::TruncatedRow)?;
    let slice = bytes.get(*cursor..end).ok_or(TupleError::TruncatedRow)?;
    *cursor = end;
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use crate::{
        page::Row,
        schema::{ColumnId, ColumnMetadata, DataType},
    };

    use super::{NOT_NULL_MARKER, NULL_MARKER, TupleError, Value, decode, encode};

    fn one_column(data_type: DataType) -> Vec<ColumnMetadata> {
        vec![ColumnMetadata::new(
            ColumnId::new(1),
            "value".to_owned(),
            data_type,
        )]
    }

    #[test]
    fn 모든_값_형식을_encode후_decode한다() {
        let columns = vec![
            ColumnMetadata::new(ColumnId::new(1), "int_value".to_owned(), DataType::Int),
            ColumnMetadata::new(
                ColumnId::new(2),
                "bigint_value".to_owned(),
                DataType::BigInt,
            ),
            ColumnMetadata::new(ColumnId::new(3), "true_value".to_owned(), DataType::Boolean),
            ColumnMetadata::new(
                ColumnId::new(4),
                "false_value".to_owned(),
                DataType::Boolean,
            ),
            ColumnMetadata::new(ColumnId::new(5), "name".to_owned(), DataType::Varchar),
            ColumnMetadata::new(ColumnId::new(6), "nickname".to_owned(), DataType::Varchar),
        ];
        let values = vec![
            Value::Int(-1),
            Value::BigInt(1),
            Value::Boolean(true),
            Value::Boolean(false),
            Value::Varchar("김".to_owned()),
            Value::Null,
        ];

        let row = encode(&values, &columns).expect("값을 Row로 변환해야 함");

        assert_eq!(decode(&row, &columns), Ok(values));
    }

    #[test]
    fn 빈_row를_decode하면_잘린_row_오류를_반환한다() {
        let columns = one_column(DataType::Int);
        let row = Row::from_bytes(&[]);

        assert_eq!(decode(&row, &columns), Err(TupleError::TruncatedRow));
    }

    #[test]
    fn 잘못된_null_marker를_decode하면_오류를_반환한다() {
        let columns = one_column(DataType::Int);
        let row = Row::from_bytes(&[2]);

        assert_eq!(decode(&row, &columns), Err(TupleError::InvalidNullMarker));
    }

    #[test]
    fn 잘못된_boolean_bytes를_decode하면_오류를_반환한다() {
        let columns = one_column(DataType::Boolean);
        let row = Row::from_bytes(&[NOT_NULL_MARKER, 2]);

        assert_eq!(decode(&row, &columns), Err(TupleError::InvalidBoolean));
    }

    #[test]
    fn 잘린_varchar를_decode하면_오류를_반환한다() {
        let columns = one_column(DataType::Varchar);
        let row = Row::from_bytes(&[NOT_NULL_MARKER, 0, 2, b'a']);

        assert_eq!(decode(&row, &columns), Err(TupleError::TruncatedRow));
    }

    #[test]
    fn 잘못된_utf8_varchar를_decode하면_오류를_반환한다() {
        let columns = one_column(DataType::Varchar);
        let row = Row::from_bytes(&[NOT_NULL_MARKER, 0, 1, 0xff]);

        assert_eq!(decode(&row, &columns), Err(TupleError::InvalidUtf8));
    }

    #[test]
    fn 남는_bytes가_있는_row를_decode하면_오류를_반환한다() {
        let columns = one_column(DataType::Int);
        let row = Row::from_bytes(&[NULL_MARKER, 1]);

        assert_eq!(decode(&row, &columns), Err(TupleError::TrailingBytes));
    }
}
