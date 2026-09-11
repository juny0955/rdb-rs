use thiserror::Error;

use crate::{
    index::btree::{
        BTreeKeyType,
        header::{BTreePageHeader, LEAF_HEADER_SIZE},
        leaf::LeafEntry,
    },
    storage::page::Page,
};

#[derive(Debug, Error)]
pub enum LeafPageError {
    #[error("leaf page가 손상되었습니다")]
    InvalidPage,
    #[error("leaf page에 공간이 없습니다")]
    PageFull,
}

pub fn initialize_leaf_page(page: &mut Page) {
    page.as_bytes_mut().fill(0);
    BTreePageHeader::new_leaf().write_to_page(page);
}

pub fn append_leaf_entry(page: &mut Page, entry: &LeafEntry) -> Result<(), LeafPageError> {
    let mut header = BTreePageHeader::read_from_page(page).ok_or(LeafPageError::InvalidPage)?;
    if !header.is_leaf() {
        return Err(LeafPageError::InvalidPage);
    }

    let entry_bytes = entry.to_bytes().ok_or(LeafPageError::InvalidPage)?;
    let start = header.entry_end() as usize;
    let end = start
        .checked_add(entry_bytes.len())
        .ok_or(LeafPageError::InvalidPage)?;
    if end > page.as_bytes().len() {
        return Err(LeafPageError::PageFull);
    }

    header
        .append_entry(u16::try_from(entry_bytes.len()).map_err(|_| LeafPageError::InvalidPage)?)
        .ok_or(LeafPageError::PageFull)?;
    page.as_bytes_mut()[start..end].copy_from_slice(&entry_bytes);
    header.write_to_page(page);
    Ok(())
}

pub fn read_leaf_entries(
    page: &Page,
    key_type: BTreeKeyType,
) -> Result<Vec<LeafEntry>, LeafPageError> {
    let header = BTreePageHeader::read_from_page(page).ok_or(LeafPageError::InvalidPage)?;
    if !header.is_leaf() {
        return Err(LeafPageError::InvalidPage);
    }

    let page_bytes = page.as_bytes();
    if header.entry_end() as usize > page_bytes.len() {
        return Err(LeafPageError::InvalidPage);
    }
    let mut entries = Vec::new();
    let mut offset = LEAF_HEADER_SIZE as usize;
    for _ in 0..header.entry_count() {
        if offset + 2 > header.entry_end() as usize {
            return Err(LeafPageError::InvalidPage);
        }

        let key_len = u16::from_be_bytes([page_bytes[offset], page_bytes[offset + 1]]);
        let entry_end = offset
            .checked_add(2)
            .ok_or(LeafPageError::InvalidPage)?
            .checked_add(key_len as usize)
            .ok_or(LeafPageError::InvalidPage)?
            .checked_add(10)
            .ok_or(LeafPageError::InvalidPage)?;
        if entry_end > header.entry_end() as usize {
            return Err(LeafPageError::InvalidPage);
        }

        entries.push(
            LeafEntry::from_bytes(key_type, &page_bytes[offset..entry_end])
                .ok_or(LeafPageError::InvalidPage)?,
        );
        offset = entry_end;
    }

    if offset != header.entry_end() as usize {
        return Err(LeafPageError::InvalidPage);
    }

    Ok(entries)
}
