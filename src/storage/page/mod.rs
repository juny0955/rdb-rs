mod error;
mod free_space;
mod header;
mod pager;
mod row;
mod slot;
mod types;

use crate::storage::page::header::HEADER_SIZE;
pub use error::{PageError, PagerError};
use free_space::row_allocation_size;
pub use pager::{allocate_page, page_count, read_page, write_page};
use slot::Slot;
pub use slot::SlotId;
pub use types::{PageId, Row, RowId};

const PAGE_SIZE: usize = 8192;
const SLOT_SIZE: usize = 4;
const FREE_BLOCK_SIZE: usize = 4;

#[derive(Debug)]
pub struct Page {
    data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new() -> Self {
        let mut page = Self {
            data: [0u8; PAGE_SIZE],
        };

        page.set_free_start(HEADER_SIZE as u16);
        page.set_free_end(PAGE_SIZE as u16);
        page.set_free_list_head(u16::MAX);
        page
    }

    pub fn new_raw() -> Self {
        Self {
            data: [0u8; PAGE_SIZE],
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
use pager::page_offset;
