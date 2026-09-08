use super::*;

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
        .unreferenced();

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
        .unreferenced();

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
        .unreferenced();

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
        .unreferenced();

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
        .unreferenced();

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
