use crate::{
    index::{
        header::BTreePageHeader,
        key::{BTreeKey, BTreeKeyType},
    },
    page::{Page, RowId},
};

pub(crate) fn initialize_leaf_page(page: &mut Page) {
    page.as_bytes_mut().fill(0);
    BTreePageHeader::new().write_to_page(page);
}

pub(crate) struct LeafEntry {
    key: BTreeKey,
    row_id: RowId,
}

impl LeafEntry {
    pub(crate) fn new(key: BTreeKey, row_id: RowId) -> Self {
        Self { key, row_id }
    }

    pub(crate) fn from_bytes(key_type: BTreeKeyType, bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }
        let key_len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;

        if bytes.len() != 2 + key_len + 10 {
            return None;
        }

        let key_end = 2 + key_len;
        let key_bytes = &bytes[2..key_end];
        let key = match key_type {
            BTreeKeyType::Int => BTreeKey::Int(i32::from_be_bytes(key_bytes.try_into().ok()?)),
            BTreeKeyType::BigInt => {
                BTreeKey::BigInt(i64::from_be_bytes(key_bytes.try_into().ok()?))
            }
            BTreeKeyType::Boolean => match key_bytes {
                [0] => BTreeKey::Boolean(false),
                [1] => BTreeKey::Boolean(true),
                _ => return None,
            },
            BTreeKeyType::Varchar => {
                let Ok(value) = String::from_utf8(key_bytes.to_vec()) else {
                    return None;
                };

                BTreeKey::Varchar(value)
            }
        };

        let row_id_start = 2 + key_len;
        let row_id_bytes: [u8; 10] = bytes[row_id_start..].try_into().ok()?;
        let row_id = RowId::from_bytes(row_id_bytes);

        Some(Self { key, row_id })
    }

    pub(crate) fn to_bytes(&self) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        let key_bytes = match &self.key {
            BTreeKey::Int(v) => v.to_be_bytes().to_vec(),
            BTreeKey::BigInt(v) => v.to_be_bytes().to_vec(),
            BTreeKey::Boolean(v) => u8::from(*v).to_be_bytes().to_vec(),
            BTreeKey::Varchar(v) => v.as_bytes().to_vec(),
        };

        let Ok(key_len) = u16::try_from(key_bytes.len()) else {
            return None;
        };

        bytes.extend_from_slice(key_len.to_be_bytes().as_ref());
        bytes.extend_from_slice(&key_bytes);
        bytes.extend_from_slice(self.row_id.to_bytes().as_ref());
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{PageId, SlotId};

    #[test]
    fn int_leaf_entry를_바이트로_직렬화한다() {
        // Given
        let entry = LeafEntry::new(
            BTreeKey::Int(42),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );

        // When
        let bytes = entry.to_bytes();

        // Then
        assert_eq!(
            bytes,
            Some(vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7,])
        );
    }

    #[test]
    fn varchar_leaf_entry를_바이트로_직렬화한다() {
        // Given
        let entry = LeafEntry::new(
            BTreeKey::Varchar(String::from("hi")),
            RowId::new(PageId::new(3), SlotId::new(7)),
        );

        // When
        let bytes = entry.to_bytes();

        // Then
        assert_eq!(
            bytes,
            Some(vec![0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 3, 0, 7])
        );
    }

    #[test]
    fn int_leaf_entry를_바이트에서_복원한다() {
        // Given
        let bytes = vec![0, 4, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

        // When
        let entry = LeafEntry::from_bytes(BTreeKeyType::Int, &bytes);

        // Then
        assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
    }

    #[test]
    fn varchar_leaf_entry를_바이트에서_복원한다() {
        // Given
        let bytes = vec![0, 2, b'h', b'i', 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

        // When
        let entry = LeafEntry::from_bytes(BTreeKeyType::Varchar, &bytes);

        // Then
        assert_eq!(entry.and_then(|entry| entry.to_bytes()), Some(bytes));
    }

    #[test]
    fn 잘못된_boolean_바이트를_거부한다() {
        // Given
        let bytes = [0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 7];

        // When
        let entry = LeafEntry::from_bytes(BTreeKeyType::Boolean, &bytes);

        // Then
        assert!(entry.is_none());
    }
}
