use super::*;
use crate::{
    index::btree::internal::{InternalEntry, append_internal_entry, initialize_internal_page},
    index::btree::leaf::{LeafEntry, append_leaf_entry, initialize_leaf_page},
    storage::file::open_rw,
    storage::page::{Page, SlotId, allocate_file_page as allocate_page, write_page},
    test_supports::TestRelationFile,
};

mod delete;
mod insert;
mod internal_split;
mod leaf_split;
mod lifecycle;
mod search;
