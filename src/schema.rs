mod column;
mod database;
mod table;

pub use column::ColumnMetadata;
pub use database::DatabaseMetadata;
pub use table::TableMetadata;

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

#[derive(Debug, PartialEq, Eq)]
pub enum SchemaError {
    DuplicateColumnName(String),
    DuplicateTableName(String),
    InvalidDataTypeTag(u8),
    ColumnNameTooLong(usize),
    TableNameTooLong(usize),
    DatabaseNameTooLong(usize),
    TooManyColumns,
    TooManyTables,
    TruncatedColumnMetadata,
    TruncatedTableMetadata,
    TruncatedDatabaseMetadata,
    InvalidColumnNameEncoding,
    InvalidTableNameEncoding,
    InvalidDatabaseNameEncoding,
    DuplicateColumnId(ColumnId),
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
