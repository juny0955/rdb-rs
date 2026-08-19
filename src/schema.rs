use std::{collections::HashSet, str::from_utf8};

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

#[derive(Debug, PartialEq, Eq)]
pub struct ColumnMetadata {
    id: ColumnId,
    name: String,
    data_type: DataType,
}

impl ColumnMetadata {
    pub fn new(id: ColumnId, name: String, data_type: DataType) -> Self {
        Self {
            id,
            name,
            data_type,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), SchemaError> {
        if bytes.len() < COLUMN_NAME_LENGTH_PREFIX_BYTES {
            return Err(SchemaError::TruncatedColumnMetadata);
        }

        let id = ColumnId(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        let name_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
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

        Ok((ColumnMetadata::new(id, name, data_type), consumed_bytes))
    }

    /// example: [0, 4] ['n', 'a', 'm', 'e'] [3]
    pub fn to_bytes(&self) -> Result<Vec<u8>, SchemaError> {
        let id_bytes = self.id.0.to_be_bytes();
        let name_bytes = self.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| SchemaError::ColumnNameTooLong(name_bytes.len()))?;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&id_bytes);
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.push(self.data_type.tag());

        Ok(bytes)
    }

    pub fn id(&self) -> ColumnId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn data_type(&self) -> DataType {
        self.data_type
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TableMetadata {
    id: TableId,
    name: String,
    columns: Vec<ColumnMetadata>,
}

impl TableMetadata {
    pub fn new(
        id: TableId,
        name: String,
        columns: Vec<ColumnMetadata>,
    ) -> Result<Self, SchemaError> {
        let mut column_ids = HashSet::new();
        for column in &columns {
            if !column_ids.insert(column.id()) {
                return Err(SchemaError::DuplicateColumnId(column.id()));
            }
        }

        let mut column_names = HashSet::new();
        for column in &columns {
            if !column_names.insert(column.name()) {
                return Err(SchemaError::DuplicateColumnName(column.name().to_owned()));
            }
        }

        Ok(Self { id, name, columns })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), SchemaError> {
        if bytes.len() < TABLE_NAME_LENGTH_PREFIX_BYTES {
            return Err(SchemaError::TruncatedTableMetadata);
        }

        let id = TableId(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        let name_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        let column_count_index = TABLE_NAME_LENGTH_PREFIX_BYTES + name_len;
        let column_start = column_count_index + 2;

        if bytes.len() < column_start {
            return Err(SchemaError::TruncatedTableMetadata);
        }
        let column_count =
            u16::from_be_bytes([bytes[column_count_index], bytes[column_count_index + 1]]) as usize;

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

        Ok((Self::new(id, name, columns)?, offset))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SchemaError> {
        let id_bytes = self.id.0.to_be_bytes();
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
        bytes.extend_from_slice(&id_bytes);
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&column_count.to_be_bytes());
        bytes.extend_from_slice(&column_bytes);

        Ok(bytes)
    }

    pub fn column(&self, name: &str) -> Option<&ColumnMetadata> {
        self.columns.iter().find(|column| column.name() == name)
    }

    pub fn column_index(&self, column_id: ColumnId) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| column.id() == column_id)
    }

    pub fn id(&self) -> TableId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn columns(&self) -> &[ColumnMetadata] {
        &self.columns
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DatabaseMetadata {
    name: String,
    tables: Vec<TableMetadata>,
}

impl DatabaseMetadata {
    pub fn new(name: String, tables: Vec<TableMetadata>) -> Result<Self, SchemaError> {
        let mut table_ids = HashSet::new();
        for table in &tables {
            if !table_ids.insert(table.id()) {
                return Err(SchemaError::DuplicateTableId(table.id()));
            }
        }

        let mut table_names = HashSet::new();
        for table in &tables {
            if !table_names.insert(table.name()) {
                return Err(SchemaError::DuplicateTableName(table.name().to_owned()));
            }
        }

        Ok(Self { name, tables })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), SchemaError> {
        if bytes.len() < DATABASE_NAME_LENGTH_PREFIX_BYTES {
            return Err(SchemaError::TruncatedDatabaseMetadata);
        }

        let name_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        let table_count_index = DATABASE_NAME_LENGTH_PREFIX_BYTES + name_len;
        let table_start = table_count_index + 2;

        if bytes.len() < table_start {
            return Err(SchemaError::TruncatedDatabaseMetadata);
        }
        let table_count =
            u16::from_be_bytes([bytes[table_count_index], bytes[table_count_index + 1]]) as usize;

        let name_bytes = &bytes[DATABASE_NAME_LENGTH_PREFIX_BYTES..table_count_index];
        let name = from_utf8(name_bytes)
            .map_err(|_| SchemaError::InvalidDatabaseNameEncoding)?
            .to_string();

        let mut offset = table_start;
        let mut tables = Vec::new();
        for _ in 0..table_count {
            let (table, used) = TableMetadata::from_bytes(&bytes[offset..])?;
            tables.push(table);
            offset += used;
        }

        Ok((Self::new(name, tables)?, offset))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SchemaError> {
        let name_bytes = self.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| SchemaError::DatabaseNameTooLong(name_bytes.len()))?;
        let table_count =
            u16::try_from(self.tables.len()).map_err(|_| SchemaError::TooManyTables)?;
        let mut table_bytes = Vec::new();
        for table in &self.tables {
            table_bytes.extend_from_slice(&table.to_bytes()?);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&table_count.to_be_bytes());
        bytes.extend_from_slice(&table_bytes);

        Ok(bytes)
    }

    pub fn table_by_id(&self, table_id: TableId) -> Option<&TableMetadata> {
        self.tables.iter().find(|table| table.id() == table_id)
    }

    pub fn table(&self, name: &str) -> Option<&TableMetadata> {
        self.tables.iter().find(|table| table.name() == name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tables(&self) -> &[TableMetadata] {
        &self.tables
    }
}

#[cfg(test)]
mod column_metadata {
    use super::*;

    #[test]
    fn 직렬화_역직렬화_테스트() -> Result<(), SchemaError> {
        let column = ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar);
        let bytes = column.to_bytes()?;
        assert_eq!(column, ColumnMetadata::from_bytes(&bytes)?.0);
        Ok(())
    }
}

#[cfg(test)]
mod table_metadata {
    use super::*;

    #[test]
    fn duplicate_column_name_테스트() {
        let col_name = "name".to_string();
        let columns = vec![
            ColumnMetadata::new(ColumnId::new(1), col_name.clone(), DataType::Varchar),
            ColumnMetadata::new(ColumnId::new(2), col_name.clone(), DataType::Varchar),
        ];

        let error = TableMetadata::new(TableId::new(1), "users".to_string(), columns)
            .expect_err("에러 발생해야함");
        assert_eq!(error, SchemaError::DuplicateColumnName("name".to_string()));
    }

    #[test]
    fn duplicate_column_id_테스트() {
        let columns = vec![
            ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar),
            ColumnMetadata::new(ColumnId::new(1), "address".to_string(), DataType::Varchar),
        ];

        let error = TableMetadata::new(TableId::new(1), "users".to_string(), columns)
            .expect_err("에러 발생해야함");

        assert_eq!(error, SchemaError::DuplicateColumnId(ColumnId::new(1)));
    }

    #[test]
    fn column_id로_컬럼_순서를_조회한다() -> Result<(), SchemaError> {
        let table = TableMetadata::new(
            TableId::new(1),
            "users".to_string(),
            vec![
                ColumnMetadata::new(ColumnId::new(10), "id".to_string(), DataType::BigInt),
                ColumnMetadata::new(ColumnId::new(20), "name".to_string(), DataType::Varchar),
            ],
        )?;

        assert_eq!(table.column_index(ColumnId::new(10)), Some(0));
        assert_eq!(table.column_index(ColumnId::new(20)), Some(1));
        assert_eq!(table.column_index(ColumnId::new(30)), None);

        Ok(())
    }

    #[test]
    fn 직렬화_역직렬화_테스트() -> Result<(), SchemaError> {
        let column = ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar);
        let table = TableMetadata::new(TableId::new(1), "users".to_string(), vec![column])?;
        let bytes = table.to_bytes()?;
        assert_eq!(table, TableMetadata::from_bytes(&bytes)?.0);
        Ok(())
    }
}

#[cfg(test)]
mod database_metadata {
    use super::*;

    #[test]
    fn duplicate_table_name_테스트() -> Result<(), SchemaError> {
        let columns1 = vec![
            ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar),
            ColumnMetadata::new(ColumnId::new(2), "address".to_string(), DataType::Varchar),
        ];
        let columns2 = vec![
            ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar),
            ColumnMetadata::new(ColumnId::new(2), "address".to_string(), DataType::Varchar),
        ];

        let tables = vec![
            TableMetadata::new(TableId::new(1), "users".to_string(), columns1)?,
            TableMetadata::new(TableId::new(2), "users".to_string(), columns2)?,
        ];
        let error = DatabaseMetadata::new("mydb".to_string(), tables).expect_err("에러 발생해야함");
        assert_eq!(error, SchemaError::DuplicateTableName("users".to_string()));

        Ok(())
    }

    #[test]
    fn duplicate_table_id_테스트() -> Result<(), SchemaError> {
        let users = TableMetadata::new(
            TableId::new(1),
            "users".to_string(),
            vec![ColumnMetadata::new(
                ColumnId::new(1),
                "name".to_string(),
                DataType::Varchar,
            )],
        )?;
        let orders = TableMetadata::new(
            TableId::new(1),
            "orders".to_string(),
            vec![ColumnMetadata::new(
                ColumnId::new(1),
                "amount".to_string(),
                DataType::Int,
            )],
        )?;

        let error = DatabaseMetadata::new("mydb".to_string(), vec![users, orders])
            .expect_err("에러 발생해야함");

        assert_eq!(error, SchemaError::DuplicateTableId(TableId::new(1)));
        Ok(())
    }

    #[test]
    fn 직렬화_역직렬화_테스트() -> Result<(), SchemaError> {
        let column = ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar);
        let table = TableMetadata::new(TableId::new(1), "users".to_string(), vec![column])?;
        let database = DatabaseMetadata::new("mydb".to_string(), vec![table])?;
        let bytes = database.to_bytes()?;
        assert_eq!(database, DatabaseMetadata::from_bytes(&bytes)?.0);
        Ok(())
    }
}
