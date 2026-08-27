use std::{collections::HashMap, fs::File, io, path::PathBuf};

use thiserror::Error;

use crate::{
    buffer::{
        frame::{BufferFrame, FrameId},
        page_key::PageKey,
        page_table::PageTable,
    },
    file::open_rw,
    page::{Page, PagerError, read_page, write_page},
    schema::TableId,
};

#[derive(Debug, Error)]
pub enum BufferPoolError {
    #[error("빈 buffer frame이 없습니다")]
    NoFreeFrame,
    #[error("이미 buffer pool에 있는 page입니다")]
    PageAlreadyCached,
    #[error("이미 unpin된 page 입니다")]
    AlreadyUnpinned,
    #[error("cache 되지 않은 page 입니다")]
    PageNotCached,
    #[error("pin 상태인 page 입니다")]
    PagePinned,
    #[error("등록되지 않은 table 입니다: {0:?}")]
    TableNotRegistered(TableId),
    #[error("table file I/O 오류: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Pager(#[from] PagerError),
}

pub(crate) struct FrameGuard<'a> {
    frame: &'a mut BufferFrame,
}

impl<'a> FrameGuard<'a> {
    fn new(frame: &'a mut BufferFrame) -> Self {
        Self { frame }
    }

    pub(crate) fn flush(&mut self, file: &mut File) -> Result<(), BufferPoolError> {
        if self.frame.is_dirty() {
            write_page(file, self.frame.page_key().page_id(), self.frame.page())?;
            self.frame.mark_clean();
        }

        Ok(())
    }

    pub(crate) fn page(&self) -> &Page {
        self.frame.page()
    }

    pub(crate) fn page_mut(&mut self) -> &mut Page {
        self.frame.page_mut()
    }
}

impl<'a> Drop for FrameGuard<'a> {
    fn drop(&mut self) {
        let _ = self.frame.unpin();
    }
}

#[derive(Debug)]
pub struct BufferPool {
    frames: Vec<Option<BufferFrame>>,
    page_table: PageTable,
    table_paths: HashMap<TableId, PathBuf>,
    hand_index: usize,
}

impl BufferPool {
    pub(crate) fn new(capacity: usize) -> Self {
        let mut frames = Vec::new();
        frames.resize_with(capacity, || None);
        Self {
            frames,
            page_table: PageTable::new(),
            table_paths: HashMap::new(),
            hand_index: 0,
        }
    }

    pub(crate) fn register_table(&mut self, table_id: TableId, path: PathBuf) {
        self.table_paths.insert(table_id, path);
    }

    pub(crate) fn fetch_page(
        &mut self,
        page_key: PageKey,
    ) -> Result<FrameGuard<'_>, BufferPoolError> {
        if self.page_table.get(&page_key).is_some() {
            let frame = self
                .fetch_cached_frame(page_key)
                .ok_or(BufferPoolError::PageNotCached)?;

            return Ok(FrameGuard::new(frame));
        }

        if self.frames.iter().all(Option::is_some) && self.evict_clock_victim()?.is_none() {
            return Err(BufferPoolError::NoFreeFrame);
        }

        let mut file = self.open_table_file(page_key.table_id())?;
        let page = read_page(&mut file, page_key.page_id())?;
        let frame_id = self.insert_frame(BufferFrame::new(page_key, page))?;

        let frame = self.get_frame_mut(frame_id)?;
        Ok(FrameGuard::new(frame))
    }

    fn insert_frame(&mut self, frame: BufferFrame) -> Result<FrameId, BufferPoolError> {
        for (index, f) in self.frames.iter().enumerate() {
            if f.is_none() {
                let frame_id = FrameId::new(index);
                let page_key = frame.page_key();

                self.page_table
                    .insert(page_key, frame_id)
                    .map_err(|_| BufferPoolError::PageAlreadyCached)?;
                self.frames[index] = Some(frame);

                return Ok(frame_id);
            }
        }

        Err(BufferPoolError::NoFreeFrame)
    }

    fn fetch_cached_frame(&mut self, page_key: PageKey) -> Option<&mut BufferFrame> {
        let frame_id = self.page_table.get(&page_key)?;
        let frame = self.frames.get_mut(frame_id.index())?.as_mut()?;
        frame.pin();
        Some(frame)
    }

    pub(crate) fn flush_page(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let mut file = self.open_table_file(page_key.table_id())?;
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;

        if frame.is_dirty() {
            write_page(&mut file, page_key.page_id(), frame.page())?;
            frame.mark_clean();
        }

        Ok(())
    }

    pub(crate) fn evict_clock_victim(&mut self) -> Result<Option<PageKey>, BufferPoolError> {
        let Some(frame_id) = self.select_clock_victim() else {
            return Ok(None);
        };

        let page_key = self.get_frame_mut(frame_id)?.page_key();
        self.evict_page(page_key)?;
        Ok(Some(page_key))
    }

    fn evict_page(&mut self, page_key: PageKey) -> Result<(), BufferPoolError> {
        let frame_id = self.get_frame_id(page_key)?;
        let frame = self.get_frame_mut(frame_id)?;

        if frame.pin_count() > 0 {
            return Err(BufferPoolError::PagePinned);
        }

        if frame.is_dirty() {
            self.flush_page(page_key)?;
        }

        self.frames[frame_id.index()]
            .take()
            .ok_or(BufferPoolError::PageNotCached)?;
        self.page_table
            .remove(&page_key)
            .ok_or(BufferPoolError::PageNotCached)?;

        Ok(())
    }

    fn get_frame_id(&self, page_key: PageKey) -> Result<FrameId, BufferPoolError> {
        self.page_table
            .get(&page_key)
            .ok_or(BufferPoolError::PageNotCached)
    }

    fn get_frame_mut(&mut self, frame_id: FrameId) -> Result<&mut BufferFrame, BufferPoolError> {
        self.frames
            .get_mut(frame_id.index())
            .ok_or(BufferPoolError::PageNotCached)?
            .as_mut()
            .ok_or(BufferPoolError::PageNotCached)
    }

    fn select_clock_victim(&mut self) -> Option<FrameId> {
        if self.frames.is_empty() {
            return None;
        }

        for _ in 0..(self.frames.len() * 2) {
            let index = self.hand_index;
            self.hand_index = (index + 1) % self.frames.len();

            let frame_id = FrameId::new(index);
            let Some(frame) = self.frames.get_mut(index).and_then(|slot| slot.as_mut()) else {
                continue;
            };

            if frame.pin_count() > 0 {
                continue;
            }

            if frame.is_referenced() {
                frame.unreference();
                continue;
            }

            return Some(frame_id);
        }

        None
    }

    fn open_table_file(&self, table_id: TableId) -> Result<File, BufferPoolError> {
        let path = self
            .table_paths
            .get(&table_id)
            .ok_or(BufferPoolError::TableNotRegistered(table_id))?;
        Ok(open_rw(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Page, PageId, PagerError, Row, allocate_page, read_page, write_page};
    use crate::schema::TableId;
    use tempfile::NamedTempFile;

    fn key(page_id: u64) -> PageKey {
        PageKey::new(TableId::new(1), PageId::new(page_id))
    }

    fn page_key(page_id: PageId) -> PageKey {
        PageKey::new(TableId::new(1), page_id)
    }

    fn frame(page_id: u64) -> BufferFrame {
        BufferFrame::new(key(page_id), Page::new())
    }

    fn register_test_file(pool: &mut BufferPool, test_file: &NamedTempFile) {
        pool.register_table(TableId::new(1), test_file.path().to_path_buf());
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

    #[test]
    fn capacity만큼_빈_frame을_생성한다() {
        let pool = BufferPool::new(2);

        assert_eq!(pool.frames.len(), 2);
        assert!(pool.frames.iter().all(|frame| frame.is_none()));
    }

    #[test]
    fn 빈_frame에_순서대로_삽입한다() {
        let mut pool = BufferPool::new(2);

        assert!(matches!(
            pool.insert_frame(frame(0)),
            Ok(frame_id) if frame_id == FrameId::new(0)
        ));
        assert!(matches!(
            pool.insert_frame(frame(1)),
            Ok(frame_id) if frame_id == FrameId::new(1)
        ));
    }

    #[test]
    fn 삽입한_frame을_page_table에서_조회한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);

        let frame_id = pool
            .insert_frame(BufferFrame::new(page_key, Page::new()))
            .expect("빈 frame에 삽입해야 한다");

        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn 중복_page_key_삽입은_slot을_변경하지_않는다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(BufferFrame::new(page_key, Page::new()))
            .expect("첫 삽입은 성공해야 한다");

        assert!(matches!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new())),
            Err(BufferPoolError::PageAlreadyCached)
        ));
        assert_eq!(pool.page_table.get(&page_key), Some(first_frame_id));
        assert!(pool.frames[1].is_none());
    }

    #[test]
    fn cache_hit이면_frame을_pin한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );

        let frame = pool
            .fetch_cached_frame(page_key)
            .expect("삽입한 page는 cache hit여야 한다");

        assert_eq!(frame.pin_count(), 2);
    }

    #[test]
    fn cache_miss는_pool_상태를_변경하지_않는다() {
        let cached_page_key = key(0);
        let missing_page_key = key(1);
        let mut pool = BufferPool::new(2);
        let frame_id = pool
            .insert_frame(BufferFrame::new(cached_page_key, Page::new()))
            .expect("첫 삽입은 성공해야 한다");

        assert!(pool.fetch_cached_frame(missing_page_key).is_none());
        assert_eq!(pool.page_table.get(&cached_page_key), Some(frame_id));
        assert!(pool.page_table.get(&missing_page_key).is_none());
        assert!(pool.frames[0].is_some());
        assert!(pool.frames[1].is_none());
    }

    #[test]
    fn frame_guard_drop은_pin_count를_감소시킨다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(BufferFrame::new(page_key, Page::new()))
            .expect("frame을 삽입해야 한다");

        {
            let frame = pool.get_frame_mut(frame_id).expect("frame이 있어야 한다");
            let _guard = FrameGuard::new(frame);
        }

        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
    }

    #[test]
    fn 등록되지_않은_table의_page를_fetch하면_오류다() {
        let mut pool = BufferPool::new(1);

        assert!(matches!(
            pool.fetch_page(key(0)),
            Err(BufferPoolError::TableNotRegistered(id)) if id == TableId::new(1)
        ));
    }

    #[test]
    fn 다른_table의_같은_page_id는_서로_다른_frame에_저장된다()
    -> Result<(), Box<dyn std::error::Error>> {
        let first_table_id = TableId::new(1);
        let second_table_id = TableId::new(2);
        let first_file = NamedTempFile::new()?;
        let second_file = NamedTempFile::new()?;
        let mut first_table_file = first_file.reopen()?;
        let mut second_table_file = second_file.reopen()?;
        let page_id = allocate_page(&mut first_table_file)?;
        assert_eq!(allocate_page(&mut second_table_file)?, page_id);

        let first_row = Row::from_bytes(b"first table");
        let mut first_page = Page::new();
        let first_slot_id = first_page.insert_row(&first_row)?;
        write_page(&mut first_table_file, page_id, &first_page)?;

        let second_row = Row::from_bytes(b"second table");
        let mut second_page = Page::new();
        let second_slot_id = second_page.insert_row(&second_row)?;
        write_page(&mut second_table_file, page_id, &second_page)?;

        let first_key = PageKey::new(first_table_id, page_id);
        let second_key = PageKey::new(second_table_id, page_id);
        let mut pool = BufferPool::new(2);
        pool.register_table(first_table_id, first_file.path().to_path_buf());
        pool.register_table(second_table_id, second_file.path().to_path_buf());

        {
            let first_frame = pool.fetch_page(first_key)?;
            assert_eq!(first_frame.page().read_row(first_slot_id)?, first_row);
        }
        {
            let second_frame = pool.fetch_page(second_key)?;
            assert_eq!(second_frame.page().read_row(second_slot_id)?, second_row);
        }

        assert_ne!(
            pool.get_frame_id(first_key)?,
            pool.get_frame_id(second_key)?
        );
        Ok(())
    }

    #[test]
    fn 다른_table의_dirty_victim을_evict하면_원래_file에_기록한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let first_table_id = TableId::new(1);
        let second_table_id = TableId::new(2);
        let first_file = NamedTempFile::new()?;
        let second_file = NamedTempFile::new()?;
        let mut first_table_file = first_file.reopen()?;
        let mut second_table_file = second_file.reopen()?;
        let page_id = allocate_page(&mut first_table_file)?;
        assert_eq!(allocate_page(&mut second_table_file)?, page_id);

        let first_key = PageKey::new(first_table_id, page_id);
        let second_key = PageKey::new(second_table_id, page_id);
        let mut pool = BufferPool::new(1);
        pool.register_table(first_table_id, first_file.path().to_path_buf());
        pool.register_table(second_table_id, second_file.path().to_path_buf());

        let row = Row::from_bytes(b"dirty victim");
        let slot_id = {
            let mut first_frame = pool.fetch_page(first_key)?;
            first_frame.page_mut().insert_row(&row)?
        };

        {
            let _second_frame = pool.fetch_page(second_key)?;
        }

        assert_eq!(pool.page_table.get(&first_key), None);
        assert!(pool.page_table.get(&second_key).is_some());
        let persisted_page = read_page(&mut first_table_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);
        Ok(())
    }

    #[test]
    fn cache_hit_guard가_drop되면_기존_pin_count로_돌아온다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );

        {
            let _guard = pool
                .fetch_cached_frame(page_key)
                .map(FrameGuard::new)
                .expect("cache hit이어야 한다");
        }
        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 1);
    }

    #[test]
    fn 수정한_frame은_unpin후에도_dirty다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        assert!(
            pool.insert_frame(BufferFrame::new(page_key, Page::new()))
                .is_ok()
        );
        unpin_frame(&mut pool, page_key);

        {
            let frame = pool
                .fetch_cached_frame(page_key)
                .expect("삽입한 page는 cache hit여야 한다");
            assert!(!frame.is_dirty());
            let _ = frame.page_mut();
            assert!(frame.is_dirty());
        }

        unpin_frame(&mut pool, page_key);
        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert_eq!(frame.pin_count(), 0);
        assert!(frame.is_dirty());
    }

    #[test]
    fn 가득찬_pool에_삽입하면_기존_frame을_유지한다() {
        let mut pool = BufferPool::new(1);
        assert!(pool.insert_frame(frame(0)).is_ok());

        assert!(matches!(
            pool.insert_frame(frame(1)),
            Err(BufferPoolError::NoFreeFrame)
        ));
        assert!(pool.frames[0].is_some());
    }

    #[test]
    fn dirty_frame을_flush하면_disk에_기록하고_clean으로_표시한다()
    -> Result<(), Box<dyn std::error::Error>> {
        let test_file = NamedTempFile::new()?;
        let mut file = test_file.reopen()?;
        let page_id = allocate_page(&mut file)?;
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let row = Row::from_bytes(b"flush");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        register_test_file(&mut pool, &test_file);
        pool.insert_frame(frame)?;

        pool.flush_page(page_key)?;

        let frame = pool.frames[0].as_ref().expect("frame이 있어야 한다");
        assert!(!frame.is_dirty());
        assert_eq!(frame.pin_count(), 1);

        drop(file);
        let mut reopened_file = test_file.reopen()?;
        let persisted_page = read_page(&mut reopened_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);

        Ok(())
    }

    #[test]
    fn flush가_실패하면_dirty_상태를_유지한다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let page_id = PageId::new(0);
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let _ = frame.page_mut();
        let mut pool = BufferPool::new(1);
        register_test_file(&mut pool, &test_file);
        assert!(pool.insert_frame(frame).is_ok());

        assert!(matches!(
            pool.flush_page(page_key),
            Err(BufferPoolError::Pager(PagerError::PageNotAllocated(id))) if id == page_id
        ));
        assert!(
            pool.frames[0]
                .as_ref()
                .expect("frame이 있어야 한다")
                .is_dirty()
        );
    }

    #[test]
    fn clean_unpinned_page를_evict하면_frame을_재사용할_수_있다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");
        unpin_frame(&mut pool, page_key);

        pool.evict_page(page_key)
            .expect("clean unpinned page는 제거해야 한다");

        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);
        assert_eq!(
            pool.insert_frame(frame(1))
                .expect("제거한 frame을 재사용해야 한다"),
            frame_id
        );
    }

    #[test]
    fn pinned_page는_evict하지_않고_상태를_유지한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");

        assert!(matches!(
            pool.evict_page(page_key),
            Err(BufferPoolError::PagePinned)
        ));
        assert!(pool.frames[frame_id.index()].is_some());
        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn dirty_unpinned_page를_evict하면_disk에_기록한다() -> Result<(), Box<dyn std::error::Error>> {
        let test_file = NamedTempFile::new()?;
        let mut file = test_file.reopen()?;
        let page_id = allocate_page(&mut file)?;
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let row = Row::from_bytes(b"evict");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        register_test_file(&mut pool, &test_file);
        let frame_id = pool.insert_frame(frame)?;
        unpin_frame(&mut pool, page_key);

        pool.evict_page(page_key)?;

        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);

        drop(file);
        let mut reopened_file = test_file.reopen()?;
        let persisted_page = read_page(&mut reopened_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);

        Ok(())
    }

    #[test]
    fn dirty_page의_flush가_실패하면_evict하지_않는다() {
        let test_file = NamedTempFile::new().expect("임시 파일을 생성해야 한다");
        let page_id = PageId::new(0);
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let _ = frame.page_mut();
        let mut pool = BufferPool::new(1);
        register_test_file(&mut pool, &test_file);
        let frame_id = pool.insert_frame(frame).expect("빈 frame에 삽입해야 한다");
        unpin_frame(&mut pool, page_key);

        assert!(matches!(
            pool.evict_page(page_key),
            Err(BufferPoolError::Pager(PagerError::PageNotAllocated(id))) if id == page_id
        ));
        assert!(
            pool.frames[frame_id.index()]
                .as_ref()
                .expect("flush 실패 후 frame이 유지되어야 한다")
                .is_dirty()
        );
        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }

    #[test]
    fn clock은_unpinned_reference_false_frame을_선택한다() {
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");
        unpin_frame(&mut pool, key(0));
        pool.frames[frame_id.index()]
            .as_mut()
            .expect("frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.select_clock_victim(), Some(frame_id));
    }

    #[test]
    fn clock은_reference_true_frame에_second_chance를_준다() {
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(frame(0))
            .expect("첫 frame을 삽입해야 한다");
        let second_frame_id = pool
            .insert_frame(frame(1))
            .expect("두 번째 frame을 삽입해야 한다");
        unpin_frame(&mut pool, key(0));
        unpin_frame(&mut pool, key(1));
        pool.frames[second_frame_id.index()]
            .as_mut()
            .expect("두 번째 frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.select_clock_victim(), Some(second_frame_id));
        assert!(
            !pool.frames[first_frame_id.index()]
                .as_ref()
                .expect("첫 frame이 있어야 한다")
                .is_referenced()
        );
    }

    #[test]
    fn clock은_pinned_frame을_건너뛴다() {
        let mut pool = BufferPool::new(2);
        let pinned_frame_id = pool
            .insert_frame(frame(0))
            .expect("첫 frame을 삽입해야 한다");
        let candidate_frame_id = pool
            .insert_frame(frame(1))
            .expect("두 번째 frame을 삽입해야 한다");
        unpin_frame(&mut pool, key(1));
        pool.frames[candidate_frame_id.index()]
            .as_mut()
            .expect("후보 frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.select_clock_victim(), Some(candidate_frame_id));
        assert!(
            pool.frames[pinned_frame_id.index()]
                .as_ref()
                .expect("pinned frame이 있어야 한다")
                .is_referenced()
        );
    }

    #[test]
    fn clock은_capacity_0이면_none을_반환한다() {
        let mut pool = BufferPool::new(0);

        assert_eq!(pool.select_clock_victim(), None);
    }

    #[test]
    fn clock은_모든_frame이_pinned이면_none을_반환한다() {
        let mut pool = BufferPool::new(2);
        let first_frame_id = pool
            .insert_frame(frame(0))
            .expect("첫 frame을 삽입해야 한다");
        let second_frame_id = pool
            .insert_frame(frame(1))
            .expect("두 번째 frame을 삽입해야 한다");

        assert_eq!(pool.select_clock_victim(), None);
        assert!(
            pool.frames[first_frame_id.index()]
                .as_ref()
                .expect("첫 frame이 있어야 한다")
                .is_referenced()
        );
        assert!(
            pool.frames[second_frame_id.index()]
                .as_ref()
                .expect("두 번째 frame이 있어야 한다")
                .is_referenced()
        );
    }

    #[test]
    fn clock_victim을_evict하면_page와_frame_매핑을_제거한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");
        unpin_frame(&mut pool, page_key);
        pool.frames[frame_id.index()]
            .as_mut()
            .expect("frame이 있어야 한다")
            .unreference();

        assert!(matches!(
            pool.evict_clock_victim(),
            Ok(Some(id)) if id == page_key
        ));
        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);
    }

    #[test]
    fn dirty_clock_victim을_evict하면_disk에_기록한다() -> Result<(), Box<dyn std::error::Error>> {
        let test_file = NamedTempFile::new()?;
        let mut file = test_file.reopen()?;
        let page_id = allocate_page(&mut file)?;
        let page_key = page_key(page_id);
        let mut frame = BufferFrame::new(page_key, Page::new());
        let row = Row::from_bytes(b"clock evict");
        let slot_id = frame.page_mut().insert_row(&row)?;
        let mut pool = BufferPool::new(1);
        register_test_file(&mut pool, &test_file);
        let frame_id = pool.insert_frame(frame)?;
        unpin_frame(&mut pool, page_key);
        pool.frames[frame_id.index()]
            .as_mut()
            .expect("frame이 있어야 한다")
            .unreference();

        assert_eq!(pool.evict_clock_victim()?, Some(page_key));
        assert!(pool.frames[frame_id.index()].is_none());
        assert_eq!(pool.page_table.get(&page_key), None);

        drop(file);
        let mut reopened_file = test_file.reopen()?;
        let persisted_page = read_page(&mut reopened_file, page_id)?;
        assert_eq!(persisted_page.read_row(slot_id)?, row);

        Ok(())
    }

    #[test]
    fn clock_victim이_없으면_pool_상태를_유지한다() {
        let page_key = key(0);
        let mut pool = BufferPool::new(1);
        let frame_id = pool
            .insert_frame(frame(0))
            .expect("빈 frame에 삽입해야 한다");

        assert!(matches!(pool.evict_clock_victim(), Ok(None)));
        assert!(pool.frames[frame_id.index()].is_some());
        assert_eq!(pool.page_table.get(&page_key), Some(frame_id));
    }
}
