use std::{collections::HashSet, str::from_utf8};

use crate::schema::{
    ColumnId, ColumnMetadata, SchemaError, TABLE_NAME_LENGTH_PREFIX_BYTES, TableId,
};

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

#[cfg(test)]
mod table_metadata {
    use crate::schema::DataType;

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
