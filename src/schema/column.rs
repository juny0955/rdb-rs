use std::str::from_utf8;

use crate::schema::{COLUMN_NAME_LENGTH_PREFIX_BYTES, ColumnId, DataType, SchemaError};

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
