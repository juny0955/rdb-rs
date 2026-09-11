use super::*;
use crate::catalog::metadata::{RelationId, TableId};
use crate::storage::page::{Page, PageId};

fn key(page_id: u64) -> PageKey {
    PageKey::new(RelationId::Heap(TableId::new(1)), PageId::new(page_id))
}

fn frame(page_id: u64) -> BufferFrame {
    BufferFrame::new(key(page_id), Page::new())
}

fn unpin_frame(pool: &mut BufferPool, page_key: PageKey) {
    let frame_id = pool
        .get_frame_id(page_key)
        .expect("frame이 cache되어 있어야 한다");
    pool.get_frame_mut(frame_id)
        .expect("frame이 있어야 한다")
        .unpin()
        .expect("pinned frame은 unpin되어야 한다");
}

mod clock;
mod eviction;
mod fetch;
mod flush;
mod frame;
