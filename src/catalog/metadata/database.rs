use std::str::from_utf8;

use super::{
    DATABASE_NAME_LENGTH_PREFIX_BYTES, IndexId, SchemaError, TableId, TableMetadata,
    index::IndexMetadata,
};

#[derive(Debug, PartialEq, Eq)]
pub struct DatabaseMetadata {
    name: String,
    tables: Vec<TableMetadata>,
    indexes: Vec<IndexMetadata>,
}

impl DatabaseMetadata {
    pub fn new(
        name: String,
        tables: Vec<TableMetadata>,
        indexes: Vec<IndexMetadata>,
    ) -> Result<Self, SchemaError> {
        let mut database_metadata = Self {
            name,
            tables: vec![],
            indexes: vec![],
        };

        for table in tables {
            database_metadata.add_table(table)?;
        }

        for index in indexes {
            database_metadata.add_index(index)?;
        }

        Ok(database_metadata)
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

    pub fn add_index(&mut self, index: IndexMetadata) -> Result<(), SchemaError> {
        if self.index_by_id(index.id()).is_some() {
            return Err(SchemaError::DuplicateIndexId(index.id()));
        }

        if self.index(index.name()).is_some() {
            return Err(SchemaError::DuplicateIndexName(index.name().to_owned()));
        }

        if let Some(table) = self.table_by_id(index.table_id()) {
            if table.column_index(index.column_id()).is_none() {
                return Err(SchemaError::IndexColumnNotFound {
                    table_id: index.table_id(),
                    column_id: index.column_id(),
                });
            }
        } else {
            return Err(SchemaError::IndexTableNotFound(index.table_id()));
        }

        self.indexes.push(index);
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

        if bytes.len() == offset {
            return Ok((Self::new(name, tables, vec![])?, offset));
        }

        if bytes.len() < offset + 2 {
            return Err(SchemaError::TruncatedDatabaseMetadata);
        }

        let index_count = u16::from_be_bytes(
            bytes[offset..offset + 2]
                .try_into()
                .map_err(|_| SchemaError::TruncatedDatabaseMetadata)?,
        ) as usize;
        offset += 2;
        let mut indexes = Vec::new();
        for _ in 0..index_count {
            let (index, used) = IndexMetadata::from_bytes(&bytes[offset..])?;
            indexes.push(index);
            offset += used;
        }

        Ok((Self::new(name, tables, indexes)?, offset))
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

        let index_count =
            u16::try_from(self.indexes.len()).map_err(|_| SchemaError::TooManyIndexes)?;
        let mut index_bytes = Vec::new();
        for index in &self.indexes {
            index_bytes.extend_from_slice(&index.to_bytes()?);
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&table_count.to_be_bytes());
        bytes.extend_from_slice(&table_bytes);
        bytes.extend_from_slice(&index_count.to_be_bytes());
        bytes.extend_from_slice(&index_bytes);

        Ok(bytes)
    }

    pub fn table_by_id(&self, table_id: TableId) -> Option<&TableMetadata> {
        self.tables.iter().find(|table| table.id() == table_id)
    }

    pub fn table(&self, name: &str) -> Option<&TableMetadata> {
        self.tables.iter().find(|table| table.name() == name)
    }

    pub fn index_by_id(&self, index_id: IndexId) -> Option<&IndexMetadata> {
        self.indexes.iter().find(|index| index.id() == index_id)
    }

    pub fn index(&self, name: &str) -> Option<&IndexMetadata> {
        self.indexes.iter().find(|index| index.name() == name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tables(&self) -> &[TableMetadata] {
        &self.tables
    }

    pub fn indexes(&self) -> &[IndexMetadata] {
        &self.indexes
    }
}

#[cfg(test)]
mod database_metadata {
    use crate::{
        catalog::metadata::{ColumnId, ColumnMetadata, DataType},
        storage::page::PageId,
    };

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
        let error =
            DatabaseMetadata::new("mydb".to_string(), tables, vec![]).expect_err("에러 발생해야함");
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

        let error = DatabaseMetadata::new("mydb".to_string(), vec![users, orders], vec![])
            .expect_err("에러 발생해야함");

        assert_eq!(error, SchemaError::DuplicateTableId(TableId::new(1)));
        Ok(())
    }

    #[test]
    fn add_table은_새_테이블을_추가한다() -> Result<(), SchemaError> {
        let mut database = DatabaseMetadata::new("mydb".to_owned(), vec![], vec![])?;

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
        let mut database =
            DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![])?;

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
        let mut database =
            DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![])?;

        let error = database
            .add_table(table(2, "users")?)
            .expect_err("중복 table 이름 오류가 발생해야 함");

        assert_eq!(error, SchemaError::DuplicateTableName("users".to_owned()));
        assert_eq!(database.tables().len(), 1);
        assert!(database.table_by_id(TableId::new(2)).is_none());
        Ok(())
    }

    #[test]
    fn new는_없는_테이블을_참조하는_index를_거부한다() -> Result<(), SchemaError> {
        let index = IndexMetadata::new(
            IndexId::new(1),
            "idx_users_id".to_owned(),
            TableId::new(999),
            ColumnId::new(1),
            PageId::new(0),
        );

        let error = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![index])
            .expect_err("존재하지 않는 인덱스 대상 테이블은 거부해야 함");

        assert_eq!(error, SchemaError::IndexTableNotFound(TableId::new(999)));
        Ok(())
    }

    #[test]
    fn new는_없는_컬럼을_참조하는_index를_거부한다() -> Result<(), SchemaError> {
        let index = IndexMetadata::new(
            IndexId::new(1),
            "idx_users_unknown".to_owned(),
            TableId::new(1),
            ColumnId::new(999),
            PageId::new(0),
        );

        let error = DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![index])
            .expect_err("존재하지 않는 인덱스 대상 컬럼은 거부해야 함");

        assert_eq!(
            error,
            SchemaError::IndexColumnNotFound {
                table_id: TableId::new(1),
                column_id: ColumnId::new(999),
            }
        );
        Ok(())
    }

    #[test]
    fn add_index는_없는_테이블을_거부하고_추가하지_않는다() -> Result<(), SchemaError> {
        let mut database =
            DatabaseMetadata::new("mydb".to_owned(), vec![table(1, "users")?], vec![])?;
        let index = IndexMetadata::new(
            IndexId::new(1),
            "idx_missing_table".to_owned(),
            TableId::new(999),
            ColumnId::new(1),
            PageId::new(0),
        );

        let error = database
            .add_index(index)
            .expect_err("존재하지 않는 인덱스 대상 테이블은 거부해야 함");

        assert_eq!(error, SchemaError::IndexTableNotFound(TableId::new(999)));
        assert!(database.index_by_id(IndexId::new(1)).is_none());
        Ok(())
    }

    #[test]
    fn 직렬화_역직렬화_테스트() -> Result<(), SchemaError> {
        let column = ColumnMetadata::new(ColumnId::new(1), "name".to_string(), DataType::Varchar);
        let table = TableMetadata::new(TableId::new(1), "users".to_string(), vec![column])?;
        let database = DatabaseMetadata::new("mydb".to_string(), vec![table], vec![])?;
        let bytes = database.to_bytes()?;
        assert_eq!(database, DatabaseMetadata::from_bytes(&bytes)?.0);
        Ok(())
    }
}
