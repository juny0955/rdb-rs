use super::*;

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
