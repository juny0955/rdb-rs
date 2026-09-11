use std::cmp::Ordering;

use crate::{
    index::btree::{
        header::BTreePageHeader,
        key::{BTreeKey, BTreeKeyType},
    },
    storage::page::{Page, PageId},
};

mod entry;
mod merge;
mod page;
mod split;
pub use entry::InternalEntry;
pub use merge::{find_leaf_merge_pair, remove_right_child_after_leaf_merge};
pub use page::{
    InternalPageError, append_internal_entry, initialize_internal_page, read_internal_entries,
};
pub use split::{internal_split, replace_child_after_leaf_split};

pub fn find_internal_child(
    page: &Page,
    key_type: BTreeKeyType,
    target: &BTreeKey,
) -> Result<PageId, InternalPageError> {
    let header =
        BTreePageHeader::from_bytes(page.as_bytes()).ok_or(InternalPageError::InvalidPage)?;
    if header.is_leaf() {
        return Err(InternalPageError::InvalidPage);
    }

    let entries = read_internal_entries(page, key_type)?;
    for entry in &entries {
        match entry.separator_key.compare(target) {
            Some(Ordering::Equal) | Some(Ordering::Greater) => return Ok(entry.left_child),
            Some(Ordering::Less) => continue,
            None => return Err(InternalPageError::InvalidPage),
        }
    }

    header
        .rightmost_child()
        .ok_or(InternalPageError::InvalidPage)
}

#[cfg(test)]
#[path = "internal/tests/mod.rs"]
mod tests;
