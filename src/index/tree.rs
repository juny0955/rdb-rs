use crate::index::key::BTreeKeyType;

pub(crate) struct BTree {
    key_type: BTreeKeyType,
}

impl BTree {
    pub(crate) fn new(key_type: BTreeKeyType) -> Self {
        Self { key_type }
    }
}