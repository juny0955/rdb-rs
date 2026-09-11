use crate::{
    index::btree::{BTreeKey, BTreeKeyType},
    storage::page::PageId,
};

pub struct InternalEntry {
    pub(super) left_child: PageId,
    pub(super) separator_key: BTreeKey,
}

impl InternalEntry {
    pub fn new(left_child: PageId, separator_key: BTreeKey) -> Self {
        Self {
            left_child,
            separator_key,
        }
    }

    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let left_bytes = self.left_child.to_bytes();
        let key_bytes = self.separator_key.to_bytes();
        let key_len = u16::try_from(key_bytes.len()).ok()?;

        bytes.extend_from_slice(left_bytes.as_ref());
        bytes.extend_from_slice(&key_len.to_be_bytes());
        bytes.extend_from_slice(&key_bytes);
        Some(bytes)
    }

    pub fn from_bytes(key_type: BTreeKeyType, bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 {
            return None;
        }
        let left_child = PageId::from_bytes(bytes[0..8].try_into().ok()?);
        let key_len = u16::from_be_bytes(bytes[8..10].try_into().ok()?) as usize;

        if bytes.len() != key_len + 10 {
            return None;
        }

        let separator_key = BTreeKey::from_bytes(key_type, &bytes[10..])?;
        Some(Self {
            left_child,
            separator_key,
        })
    }
}
