use crate::{index::key::BTreeKey, page::RowId};

pub(crate) struct LeafEntry {
    key: BTreeKey,
    row_id: RowId,
}
