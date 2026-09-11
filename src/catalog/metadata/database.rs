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

    pub fn tables(&self) -> &[TableMetadata] {
        &self.tables
    }

    pub fn indexes(&self) -> &[IndexMetadata] {
        &self.indexes
    }
}
