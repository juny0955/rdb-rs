use std::cmp::Ordering;

use crate::{
    index::btree::{
        BTreeKey, BTreeKeyType,
        leaf::{LeafPageError, append_leaf_entry, initialize_leaf_page, read_leaf_entries},
    },
    storage::page::{Page, RowId},
};

pub fn delete_leaf_entry(
    page: &mut Page,
    key_type: BTreeKeyType,
    target: &BTreeKey,
    row_id: RowId,
) -> Result<bool, LeafPageError> {
    let mut entries = read_leaf_entries(page, key_type)?;
    let Some(index) = entries
        .iter()
        .position(|entry| entry.key == *target && entry.row_id == row_id)
    else {
        return Ok(false);
    };
    entries.remove(index);

    let mut rewritten = Page::new_raw();
    initialize_leaf_page(&mut rewritten);
    for entry in entries {
        append_leaf_entry(&mut rewritten, &entry)?;
    }

    *page = rewritten;
    Ok(true)
}

pub fn find_leaf_row_ids(
    page: &Page,
    key_type: BTreeKeyType,
    target: &BTreeKey,
) -> Result<Vec<RowId>, LeafPageError> {
    let entries = read_leaf_entries(page, key_type)?;

    let mut results = Vec::new();
    for entry in &entries {
        if let Some(Ordering::Equal) = entry.key.compare(target) {
            results.push(entry.row_id);
        }
    }

    Ok(results)
}
