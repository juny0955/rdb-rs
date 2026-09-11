use crate::catalog::metadata::RelationId;
use crate::storage::page::PageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageKey(RelationId, PageId);
impl PageKey {
    pub fn new(relation_id: RelationId, page_id: PageId) -> Self {
        Self(relation_id, page_id)
    }

    pub fn relation_id(&self) -> RelationId {
        self.0
    }

    pub fn page_id(&self) -> PageId {
        self.1
    }
}
