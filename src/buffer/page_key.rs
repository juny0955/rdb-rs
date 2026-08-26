use crate::{page::PageId, schema::TableId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageKey(TableId, PageId);
impl PageKey {
    pub fn new(table_id: TableId, page_id: PageId) -> Self {
        Self(table_id, page_id)
    }

    pub fn table_id(&self) -> TableId {
        self.0
    }

    pub fn page_id(&self) -> PageId {
        self.1
    }
}
