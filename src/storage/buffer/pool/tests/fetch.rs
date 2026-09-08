use super::*;

#[test]
fn 등록되지_않은_table의_page를_fetch하면_오류다() {
    let mut pool = BufferPool::new(1);

    assert!(matches!(
        pool.fetch_page(key(0)),
        Err(BufferPoolError::RelationNotRegistered(id))
            if id == RelationId::Heap(TableId::new(1))
    ));
}

#[test]
fn 다른_table의_같은_page_id는_서로_다른_frame에_저장된다() -> Result<(), Box<dyn std::error::Error>>
{
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

    let first_relation = RelationId::Heap(first_table_id);
    let second_relation = RelationId::Heap(second_table_id);
    let first_key = PageKey::new(first_relation, page_id);
    let second_key = PageKey::new(second_relation, page_id);
    let mut pool = BufferPool::new(2);
    pool.register_relation(first_relation, first_file.path().to_path_buf());
    pool.register_relation(second_relation, second_file.path().to_path_buf());

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

    let first_relation = RelationId::Heap(first_table_id);
    let second_relation = RelationId::Heap(second_table_id);
    let first_key = PageKey::new(first_relation, page_id);
    let second_key = PageKey::new(second_relation, page_id);
    let mut pool = BufferPool::new(1);
    pool.register_relation(first_relation, first_file.path().to_path_buf());
    pool.register_relation(second_relation, second_file.path().to_path_buf());

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
