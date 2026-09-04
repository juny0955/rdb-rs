use std::str::from_utf8;

use crate::{
    schema::{ColumnId, INDEX_NAME_LENGTH_PREFIX_BYTES, IndexId, SchemaError, TableId},
    storage::page::PageId,
};

#[derive(Debug, PartialEq, Eq)]
pub struct IndexMetadata {
    id: IndexId,
    name: String,
    table_id: TableId,
    column_id: ColumnId,
    root_page_id: PageId,
}

impl IndexMetadata {
    pub fn new(
        id: IndexId,
        name: String,
        table_id: TableId,
        column_id: ColumnId,
        root_page_id: PageId,
    ) -> Self {
        Self {
            id,
            name,
            table_id,
            column_id,
            root_page_id,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, SchemaError> {
        let id_bytes = self.id.0.to_be_bytes();
        let name_bytes = self.name.as_bytes();
        let name_len = u16::try_from(name_bytes.len())
            .map_err(|_| SchemaError::IndexNameTooLong(name_bytes.len()))?;
        let table_bytes = self.table_id.0.to_be_bytes();
        let column_bytes = self.column_id.0.to_be_bytes();
        let root_page_bytes = self.root_page_id.to_bytes();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&id_bytes);
        bytes.extend_from_slice(&name_len.to_be_bytes());
        bytes.extend_from_slice(name_bytes);
        bytes.extend_from_slice(&table_bytes);
        bytes.extend_from_slice(&column_bytes);
        bytes.extend_from_slice(&root_page_bytes);
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), SchemaError> {
        if bytes.len() < INDEX_NAME_LENGTH_PREFIX_BYTES {
            return Err(SchemaError::TruncatedIndexMetadata);
        }

        let id = u32::from_be_bytes(
            bytes[0..4]
                .try_into()
                .map_err(|_| SchemaError::TruncatedIndexMetadata)?,
        );
        let name_len = u16::from_be_bytes(
            bytes[4..INDEX_NAME_LENGTH_PREFIX_BYTES]
                .try_into()
                .map_err(|_| SchemaError::TruncatedIndexMetadata)?,
        );
        let name_end = INDEX_NAME_LENGTH_PREFIX_BYTES + name_len as usize;

        let consumed = name_end + 16;
        if bytes.len() < consumed {
            return Err(SchemaError::TruncatedIndexMetadata);
        }

        let name = from_utf8(&bytes[INDEX_NAME_LENGTH_PREFIX_BYTES..name_end])
            .map_err(|_| SchemaError::InvalidIndexNameEncoding)?
            .to_string();

        let table_end = name_end + 4;
        let table_id = u32::from_be_bytes(
            bytes[name_end..table_end]
                .try_into()
                .map_err(|_| SchemaError::TruncatedIndexMetadata)?,
        );

        let column_end = table_end + 4;
        let column_id = u32::from_be_bytes(
            bytes[table_end..column_end]
                .try_into()
                .map_err(|_| SchemaError::TruncatedIndexMetadata)?,
        );

        let root_page_end = column_end + 8;
        let root_page_id = u64::from_be_bytes(
            bytes[column_end..root_page_end]
                .try_into()
                .map_err(|_| SchemaError::TruncatedIndexMetadata)?,
        );

        Ok((
            Self {
                id: IndexId(id),
                name,
                table_id: TableId(table_id),
                column_id: ColumnId(column_id),
                root_page_id: PageId::new(root_page_id),
            },
            consumed,
        ))
    }

    pub fn id(&self) -> IndexId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn table_id(&self) -> TableId {
        self.table_id
    }

    pub fn column_id(&self) -> ColumnId {
        self.column_id
    }

    pub fn root_page_id(&self) -> PageId {
        self.root_page_id
    }
}
