use super::*;
use crate::storage::file::RelationFileManagerError;

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
        Err(BufferPoolError::RelationFileManager(
            RelationFileManagerError::Pager(PagerError::PageNotAllocated(id))
        )) if id == page_id
    ));
    assert!(
        pool.frames[0]
            .as_ref()
            .expect("frame이 있어야 한다")
            .is_dirty()
    );
}
