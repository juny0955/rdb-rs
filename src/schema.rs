use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq)]
pub enum SchemaError {
    DuplicateColumnName(String),
    DuplicateTableName(String),
    InvalidDataTypeTag(u8),
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
