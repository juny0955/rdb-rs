use crate::storage::page::Page;

pub(super) const SLOT_COUNT_OFFSET: usize = 0;
pub(super) const FREE_START_OFFSET: usize = 2;
pub(super) const FREE_END_OFFSET: usize = 4;
pub(super) const FREE_LIST_HEAD_OFFSET: usize = 6;
pub(super) const HEADER_SIZE: usize = 8;

impl Page {
    pub(super) fn slot_count(&self) -> u16 {
        u16::from_be_bytes([
            self.data[SLOT_COUNT_OFFSET],
            self.data[FREE_START_OFFSET - 1],
        ])
    }

    pub(super) fn set_slot_count(&mut self, value: u16) {
        self.data[SLOT_COUNT_OFFSET..FREE_START_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub(super) fn free_start(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_START_OFFSET], self.data[FREE_END_OFFSET - 1]])
    }

    pub(super) fn set_free_start(&mut self, value: u16) {
        self.data[FREE_START_OFFSET..FREE_END_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub(super) fn free_end(&self) -> u16 {
        u16::from_be_bytes([
            self.data[FREE_END_OFFSET],
            self.data[FREE_LIST_HEAD_OFFSET - 1],
        ])
    }

    pub(super) fn set_free_end(&mut self, value: u16) {
        self.data[FREE_END_OFFSET..FREE_LIST_HEAD_OFFSET].copy_from_slice(&value.to_be_bytes());
    }

    pub(super) fn free_list_head(&self) -> u16 {
        u16::from_be_bytes([self.data[FREE_LIST_HEAD_OFFSET], self.data[HEADER_SIZE - 1]])
    }

    pub(super) fn set_free_list_head(&mut self, value: u16) {
        self.data[FREE_LIST_HEAD_OFFSET..HEADER_SIZE].copy_from_slice(&value.to_be_bytes());
    }
}
