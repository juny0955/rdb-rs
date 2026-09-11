use crate::storage::page::SlotId;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct PageId(pub(super) u64);
impl PageId {
    pub fn new(page_id: u64) -> Self {
        PageId(page_id)
    }

    pub fn to_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        PageId(u64::from_be_bytes(bytes))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RowId(pub(super) PageId, pub(super) SlotId);
impl RowId {
    pub fn new(page_id: PageId, slot_id: SlotId) -> Self {
        RowId(page_id, slot_id)
    }

    pub fn page_id(&self) -> PageId {
        self.0
    }

    pub fn slot_id(&self) -> SlotId {
        self.1
    }

    pub fn to_bytes(self) -> [u8; 10] {
        let mut bytes = [0u8; 10];
        bytes[0..8].copy_from_slice(&self.page_id().0.to_be_bytes());
        bytes[8..10].copy_from_slice(&self.slot_id().0.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: [u8; 10]) -> Self {
        let page_id = PageId::new(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]));
        let slot_id = SlotId::new(u16::from_be_bytes([bytes[8], bytes[9]]));
        Self(page_id, slot_id)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub(super) data: Vec<u8>,
}

impl Row {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: bytes.to_vec(),
        }
    }

    pub fn to_bytes(&self) -> &[u8] {
        &self.data
    }
}
