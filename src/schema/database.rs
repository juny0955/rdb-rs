use std::{collections::HashSet, str::from_utf8};

use crate::schema::{DATABASE_NAME_LENGTH_PREFIX_BYTES, SchemaError, TableId, TableMetadata};

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

    pub fn add_table(&mut self, table: TableMetadata) -> Result<(), SchemaError> {
        if self.table_by_id(table.id()).is_some() {
            return Err(SchemaError::DuplicateTableId(table.id()));
        }

        if self.table(table.name()).is_some() {
            return Err(SchemaError::DuplicateTableName(table.name().to_owned()));
        }

        self.tables.push(table);
        Ok(())
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
mod database_metadata {
    use crate::schema::{ColumnId, ColumnMetadata, DataType};

    use super::*;

    fn table(id: u32, name: &str) -> Result<TableMetadata, SchemaError> {
        TableMetadata::new(
            TableId::new(id),
            name.to_owned(),
            vec![ColumnMetadata::new(
                ColumnId::new(1),
                "id".to_owned(),
                DataType::BigInt,
            )],
        )
    }

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
    fn add_table은_새_테이블을_추가한다() -> Result<(), SchemaError> {
        let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![])?;

        database.add_table(table(1, "users")?)?;

        assert_eq!(database.tables().len(), 1);
        assert_eq!(
            database.table_by_id(TableId::new(1)).unwrap().name(),
            "users"
        );
        Ok(())
    }

    #[test]
    fn add_table은_중복_id를_거부하고_변경하지_않는다() -> Result<(), SchemaError> {
        let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?])?;

        let error = database
            .add_table(table(1, "orders")?)
            .expect_err("중복 TableId 오류가 발생해야 함");

        assert_eq!(error, SchemaError::DuplicateTableId(TableId::new(1)));
        assert_eq!(database.tables().len(), 1);
        assert!(database.table("orders").is_none());
        Ok(())
    }

    #[test]
    fn add_table은_중복_이름을_거부하고_변경하지_않는다() -> Result<(), SchemaError> {
        let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?])?;

        let error = database
            .add_table(table(2, "users")?)
            .expect_err("중복 table 이름 오류가 발생해야 함");

        assert_eq!(error, SchemaError::DuplicateTableName("users".to_owned()));
        assert_eq!(database.tables().len(), 1);
        assert!(database.table_by_id(TableId::new(2)).is_none());
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
