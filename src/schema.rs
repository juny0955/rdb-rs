use std::{collections::HashSet, str::from_utf8};

const COLUMN_NAME_LENGTH_PREFIX_BYTES: usize = 2;
const TABLE_NAME_LENGTH_PREFIX_BYTES: usize = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum SchemaError {
    DuplicateColumnName(String),
    DuplicateTableName(String),
    InvalidDataTypeTag(u8),
    ColumnNameTooLong(usize),
    TableNameTooLong(usize),
    TooManyColumns,
    TruncatedColumnMetadata,
    TruncatedTableMetadata,
    InvalidColumnNameEncoding,
    InvalidTableNameEncoding,
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

#[derive(Debug)]
pub struct ColumnMetadata {
    name: String,
    data_type: DataType,
}

impl ColumnMetadata {
    pub fn new(name: String, data_type: DataType) -> Self {
        Self { name, data_type }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), SchemaError> {
        if bytes.len() < COLUMN_NAME_LENGTH_PREFIX_BYTES {
            return Err(SchemaError::TruncatedColumnMetadata);
        }

        let name_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        let type_tag_index = COLUMN_NAME_LENGTH_PREFIX_BYTES + name_len;
        let consumed_bytes = type_tag_index + 1;

        if bytes.len() < consumed_bytes {
            return Err(SchemaError::TruncatedColumnMetadata);
        }

        let name_bytes = &bytes[COLUMN_NAME_LENGTH_PREFIX_BYTES..type_tag_index];
        let name = from_utf8(name_bytes)
            .map_err(|_| SchemaError::InvalidColumnNameEncoding)?
            .to_string();
        let data_type = DataType::from_tag(bytes[type_tag_index])?;

        Ok((ColumnMetadata::new(name, data_type), consumed_bytes))
    }

    /// example: [0, 4] ['n', 'a', 'm', 'e'] [3]
    pub fn to_bytes(&self) -> Result<Vec<u8>, SchemaError> {
        let name_bytes = self.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| SchemaError::ColumnNameTooLong(name_bytes.len()))?;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.push(self.data_type.tag());

        Ok(bytes)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn data_type(&self) -> DataType {
        self.data_type
    }
}

#[derive(Debug)]
pub struct TableMetadata {
    name: String,
    columns: Vec<ColumnMetadata>,
}

impl TableMetadata {
    pub fn new(name: String, columns: Vec<ColumnMetadata>) -> Result<Self, SchemaError> {
        let mut column_names = HashSet::new();
        for column in &columns {
            if !column_names.insert(column.name()) {
                return Err(SchemaError::DuplicateColumnName(column.name().to_owned()));
            }
        }

        Ok(Self { name, columns })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), SchemaError> {
        if bytes.len() < TABLE_NAME_LENGTH_PREFIX_BYTES {
            return Err(SchemaError::TruncatedTableMetadata);
        }

        let name_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        let column_count_index = TABLE_NAME_LENGTH_PREFIX_BYTES + name_len;
        let column_start = column_count_index + 2;

        if bytes.len() < column_start {
            return Err(SchemaError::TruncatedTableMetadata);
        }
        let column_count = u16::from_be_bytes([bytes[column_count_index], bytes[column_count_index + 1]]) as usize;

        let name_bytes = &bytes[TABLE_NAME_LENGTH_PREFIX_BYTES..column_count_index];
        let name = from_utf8(name_bytes)
            .map_err(|_| SchemaError::InvalidTableNameEncoding)?
            .to_string();

        let mut offset = column_start;
        let mut columns = Vec::new();
        for _ in 0..column_count {
            let (column, used) = ColumnMetadata::from_bytes(&bytes[offset..])?;
            columns.push(column);
            offset += used;
        }

        Ok((Self::new(name, columns)?, offset))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SchemaError> {
        let name_bytes = self.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| SchemaError::TableNameTooLong(name_bytes.len()))?;
        let column_count =
            u16::try_from(self.columns.len()).map_err(|_| SchemaError::TooManyColumns)?;
        let mut column_bytes = Vec::new();
        for column in &self.columns {
            column_bytes.extend_from_slice(&column.to_bytes()?);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&column_count.to_be_bytes());
        bytes.extend_from_slice(&column_bytes);

        Ok(bytes)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn columns(&self) -> &[ColumnMetadata] {
        &self.columns
    }
}

#[derive(Debug)]
pub struct DatabaseMetadata {
    name: String,
    tables: Vec<TableMetadata>,
}

impl DatabaseMetadata {
    pub fn new(name: String, tables: Vec<TableMetadata>) -> Result<Self, SchemaError> {
        let mut table_names = HashSet::new();
        for table in &tables {
            if !table_names.insert(table.name()) {
                return Err(SchemaError::DuplicateTableName(table.name().to_owned()));
            }
        }

        Ok(Self { name, tables })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tables(&self) -> &[TableMetadata] {
        &self.tables
    }
}

#[cfg(test)]
mod table_metadata {
    use super::*;

    #[test]
    fn duplicate_column_name_테스트() {
        let col_name = "name".to_string();
        let columns = vec![
            ColumnMetadata::new(col_name.clone(), DataType::Varchar),
            ColumnMetadata::new(col_name.clone(), DataType::Varchar),
        ];

        let error = TableMetadata::new("users".to_string(), columns).expect_err("에러 발생해야함");
        assert_eq!(error, SchemaError::DuplicateColumnName("name".to_string()));
    }
}

#[cfg(test)]
mod database_metadata {
    use super::*;

    #[test]
    fn duplicate_table_name_테스트() -> Result<(), SchemaError> {
        let columns1 = vec![
            ColumnMetadata::new("name".to_string(), DataType::Varchar),
            ColumnMetadata::new("address".to_string(), DataType::Varchar),
        ];
        let columns2 = vec![
            ColumnMetadata::new("name".to_string(), DataType::Varchar),
            ColumnMetadata::new("address".to_string(), DataType::Varchar),
        ];

        let tables = vec![
            TableMetadata::new("users".to_string(), columns1)?,
            TableMetadata::new("users".to_string(), columns2)?,
        ];
        let error = DatabaseMetadata::new("mydb".to_string(), tables).expect_err("에러 발생해야함");
        assert_eq!(error, SchemaError::DuplicateTableName("users".to_string()));

        Ok(())
    }
}
