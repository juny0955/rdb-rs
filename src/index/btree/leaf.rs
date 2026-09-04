use crate::{index::btree::header::BTreePageHeader, storage::page::Page};

mod entry;
mod operation;
mod page;
mod split;
pub use entry::LeafEntry;
pub use operation::{delete_leaf_entry, find_leaf_row_ids};
pub use page::{LeafPageError, append_leaf_entry, initialize_leaf_page, read_leaf_entries};
pub use split::{leaf_merge, leaf_split};

pub fn leaf_is_underfull(page: &Page) -> Result<bool, LeafPageError> {
    let header = BTreePageHeader::read_from_page(page).ok_or(LeafPageError::InvalidPage)?;
    if !header.is_leaf() {
        return Err(LeafPageError::InvalidPage);
    }

    if header.entry_end() as usize <= page.as_bytes().len() / 2 {
        return Ok(true);
    }

    Ok(false)
}

#[cfg(test)]
#[path = "leaf/tests/mod.rs"]
mod tests;
