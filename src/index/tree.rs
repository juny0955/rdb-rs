use crate::index::key::{BTreeKey, BTreeKeyType};

pub struct BTree {
    key_type: BTreeKeyType,
}

impl BTree {
    pub fn new(key_type: BTreeKeyType) -> Self {
        Self { key_type }
    }

    pub fn accepts_key(&self, key: &BTreeKey) -> bool {
        self.key_type == key.key_type()
    }
}
