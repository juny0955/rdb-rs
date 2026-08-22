mod column;
mod database;
mod table;

pub use column::ColumnMetadata;
pub use database::DatabaseMetadata;
pub use table::TableMetadata;
use thiserror::Error;

use crate::parser::ast;

const COLUMN_NAME_LENGTH_PREFIX_BYTES: usize = 6;
const TABLE_NAME_LENGTH_PREFIX_BYTES: usize = 6;
const DATABASE_NAME_LENGTH_PREFIX_BYTES: usize = 2;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TableId(u32);
impl TableId {
    pub fn new(value: u32) -> Self {
        Self(value)
    }
    pub fn id(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ColumnId(u32);
impl ColumnId {
    pub fn new(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum SchemaError {
    #[error("컬럼 이름이 중복됩니다: {0}")]
    DuplicateColumnName(String),
    #[error("테이블 이름이 중복됩니다: {0}")]
    DuplicateTableName(String),
    #[error("데이터 타입 태그가 올바르지 않습니다: {0}")]
    InvalidDataTypeTag(u8),
    #[error("컬럼 이름이 최대 길이를 초과했습니다: {0}바이트")]
    ColumnNameTooLong(usize),
    #[error("테이블 이름이 최대 길이를 초과했습니다: {0}바이트")]
    TableNameTooLong(usize),
    #[error("데이터베이스 이름이 최대 길이를 초과했습니다: {0}바이트")]
    DatabaseNameTooLong(usize),
    #[error("컬럼 수가 최대 개수를 초과했습니다")]
    TooManyColumns,
    #[error("테이블 수가 최대 개수를 초과했습니다")]
    TooManyTables,
    #[error("컬럼 metadata 데이터가 중간에 끝났습니다")]
    TruncatedColumnMetadata,
    #[error("테이블 metadata 데이터가 중간에 끝났습니다")]
    TruncatedTableMetadata,
    #[error("데이터베이스 metadata 데이터가 중간에 끝났습니다")]
    TruncatedDatabaseMetadata,
    #[error("컬럼 이름이 유효한 UTF-8이 아닙니다")]
    InvalidColumnNameEncoding,
    #[error("테이블 이름이 유효한 UTF-8이 아닙니다")]
    InvalidTableNameEncoding,
    #[error("데이터베이스 이름이 유효한 UTF-8이 아닙니다")]
    InvalidDatabaseNameEncoding,
    #[error("컬럼 ID가 중복됩니다: {0:?}")]
    DuplicateColumnId(ColumnId),
    #[error("테이블 ID가 중복됩니다: {0:?}")]
    DuplicateTableId(TableId),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DataType {
    Int,
    BigInt,
    Boolean,
    Varchar,
    Null,
}

impl DataType {
    pub fn tag(&self) -> u8 {
        match self {
            DataType::Int => 0,
            DataType::BigInt => 1,
            DataType::Boolean => 2,
            DataType::Varchar => 3,
            DataType::Null => 4,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Self, SchemaError> {
        match tag {
            0 => Ok(DataType::Int),
            1 => Ok(DataType::BigInt),
            2 => Ok(DataType::Boolean),
            3 => Ok(DataType::Varchar),
            4 => Ok(DataType::Null),
            _ => Err(SchemaError::InvalidDataTypeTag(tag)),
        }
    }
}

impl From<ast::DataType> for DataType {
    fn from(value: ast::DataType) -> Self {
        match value {
            ast::DataType::Int => Self::Int,
            ast::DataType::BigInt => Self::BigInt,
            ast::DataType::Boolean => Self::Boolean,
            ast::DataType::Varchar => Self::Varchar,
            ast::DataType::Null => Self::Null,
        }
    }
}
