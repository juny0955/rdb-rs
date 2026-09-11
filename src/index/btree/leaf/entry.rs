use crate::{
    index::btree::{BTreeKey, BTreeKeyType},
    storage::page::RowId,
};

#[derive(Debug)]
pub struct LeafEntry {
    pub(super) key: BTreeKey,
    pub(super) row_id: RowId,
}

impl LeafEntry {
    pub fn new(key: BTreeKey, row_id: RowId) -> Self {
        Self { key, row_id }
    }

    pub fn from_bytes(key_type: BTreeKeyType, bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }
        let key_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;

        if bytes.len() != 2 + key_len + 10 {
            return None;
        }

        let key_end = 2 + key_len;
        let key_bytes = &bytes[2..key_end];
        let key = BTreeKey::from_bytes(key_type, key_bytes)?;

        let row_id_start = 2 + key_len;
        let row_id_bytes: [u8; 10] = bytes[row_id_start..].try_into().ok()?;
        let row_id = RowId::from_bytes(row_id_bytes);

        Some(Self { key, row_id })
    }

    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let key_bytes = self.key.to_bytes();
        let key_len = u16::try_from(key_bytes.len()).ok()?;

        bytes.extend_from_slice(key_len.to_be_bytes().as_ref());
        bytes.extend_from_slice(&key_bytes);
        bytes.extend_from_slice(self.row_id.to_bytes().as_ref());
        Some(bytes)
    }
}
