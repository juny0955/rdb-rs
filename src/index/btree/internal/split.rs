use crate::{
    index::btree::{
        BTreeKey, BTreeKeyType,
        header::BTreePageHeader,
        internal::{
            InternalEntry, InternalPageError, append_internal_entry, initialize_internal_page,
            read_internal_entries,
        },
    },
    storage::page::{Page, PageId},
};

pub fn internal_split(
    left_page: &mut Page,
    right_page: &mut Page,
    key_type: BTreeKeyType,
) -> Result<BTreeKey, InternalPageError> {
    let mut entries = read_internal_entries(left_page, key_type)?;
    let header =
        BTreePageHeader::read_from_page(left_page).ok_or(InternalPageError::InvalidPage)?;

    let mut split_entries = entries.split_off(entries.len() / 2);

    if split_entries.is_empty() {
        return Err(InternalPageError::InvalidPage);
    }
    let promoted_entry = split_entries.remove(0);

    let old_rightmost = header
        .rightmost_child()
        .ok_or(InternalPageError::InvalidPage)?;
    let left_rightmost = promoted_entry.left_child;
    let separator_key = promoted_entry.separator_key;

    initialize_internal_page(left_page, left_rightmost);
    for entry in &entries {
        append_internal_entry(left_page, entry)?;
    }

    initialize_internal_page(right_page, old_rightmost);
    for entry in &split_entries {
        append_internal_entry(right_page, entry)?;
    }

    Ok(separator_key)
}

pub fn replace_child_after_leaf_split(
    parent: &mut Page,
    key_type: BTreeKeyType,
    old_child: PageId,
    left_child: PageId,
    right_child: PageId,
    separator: BTreeKey,
) -> Result<(), InternalPageError> {
    let mut entries = read_internal_entries(parent, key_type)?;
    let rightmost = BTreePageHeader::read_from_page(parent)
        .and_then(|header| header.rightmost_child())
        .ok_or(InternalPageError::InvalidPage)?;

    let new_rightmost = match entries
        .iter()
        .position(|entry| entry.left_child == old_child)
    {
        Some(index) => {
            let old_separator = entries[index].separator_key.clone();
            entries[index] = InternalEntry::new(left_child, separator);
            entries.insert(index + 1, InternalEntry::new(right_child, old_separator));
            rightmost
        }
        None if rightmost == old_child => {
            entries.push(InternalEntry::new(left_child, separator));
            right_child
        }
        None => return Err(InternalPageError::InvalidPage),
    };

    let mut rewritten = Page::new_raw();
    initialize_internal_page(&mut rewritten, new_rightmost);
    for entry in &entries {
        append_internal_entry(&mut rewritten, entry)?;
    }

    parent.as_bytes_mut().copy_from_slice(rewritten.as_bytes());
    Ok(())
}
