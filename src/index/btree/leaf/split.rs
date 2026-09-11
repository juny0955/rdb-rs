use crate::{
    index::btree::{
        BTreeKey, BTreeKeyType,
        header::BTreePageHeader,
        leaf::{
            LeafEntry, LeafPageError, append_leaf_entry, initialize_leaf_page, read_leaf_entries,
        },
    },
    storage::page::{Page, PageId},
};

pub fn leaf_split(
    left_page: &mut Page,
    right_page: &mut Page,
    right_page_id: PageId,
    key_type: BTreeKeyType,
    new_entry: LeafEntry,
) -> Result<BTreeKey, LeafPageError> {
    let mut entries = read_leaf_entries(left_page, key_type)?;
    entries.push(new_entry);

    if entries.iter().any(|entry| entry.key.key_type() != key_type) {
        return Err(LeafPageError::InvalidPage);
    }

    entries.sort_unstable_by(|left, right| {
        left.key
            .compare(&right.key)
            .expect("검증된 entry는 같은 key type이다")
    });

    let split_entries = entries.split_off(entries.len() / 2);
    let separator_key = entries
        .last()
        .ok_or(LeafPageError::InvalidPage)?
        .key
        .clone();

    let old_next = BTreePageHeader::read_from_page(left_page)
        .ok_or(LeafPageError::InvalidPage)?
        .next_leaf_page_id();
    initialize_leaf_page(left_page);
    for entry in &entries {
        append_leaf_entry(left_page, entry)?;
    }
    let mut left_page_header =
        BTreePageHeader::read_from_page(left_page).ok_or(LeafPageError::InvalidPage)?;
    left_page_header.set_next_leaf_page_id(Some(right_page_id));
    left_page_header.write_to_page(left_page);

    initialize_leaf_page(right_page);
    for entry in &split_entries {
        append_leaf_entry(right_page, entry)?;
    }

    let mut right_page_header =
        BTreePageHeader::read_from_page(right_page).ok_or(LeafPageError::InvalidPage)?;
    right_page_header.set_next_leaf_page_id(old_next);
    right_page_header.write_to_page(right_page);
    Ok(separator_key)
}

pub fn leaf_merge(
    left_page: &mut Page,
    right_page: &mut Page,
    key_type: BTreeKeyType,
) -> Result<(), LeafPageError> {
    let mut left_entries = read_leaf_entries(left_page, key_type)?;
    let mut right_entries = read_leaf_entries(right_page, key_type)?;

    left_entries.append(&mut right_entries);
    left_entries.sort_unstable_by(|left, right| {
        left.key
            .compare(&right.key)
            .expect("검증된 entry는 같은 key type이다")
    });

    let mut temp_page = Page::new();
    initialize_leaf_page(&mut temp_page);
    for entry in &left_entries {
        append_leaf_entry(&mut temp_page, entry)?;
    }
    *left_page = temp_page;

    let old_next = BTreePageHeader::read_from_page(right_page)
        .ok_or(LeafPageError::InvalidPage)?
        .next_leaf_page_id();
    let mut left_page_header =
        BTreePageHeader::read_from_page(left_page).ok_or(LeafPageError::InvalidPage)?;
    left_page_header.set_next_leaf_page_id(old_next);
    left_page_header.write_to_page(left_page);

    initialize_leaf_page(right_page);
    Ok(())
}
