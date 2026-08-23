use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum PageError {
    #[error("row가 page에 저장할 수 있는 최대 크기를 초과했습니다.")]
    RowTooLarge,
    #[error("page에 충분한 공간이 없습니다.")]
    StorageFull,
    #[error("slot을 찾을 수 없습니다.")]
    SlotNotFound,
    #[error("row 범위가 page 경계를 벗어났습니다.")]
    InvalidRowBounds,
    #[error("free space 범위가 올바르지 않습니다.")]
    InvalidFreeSpaceBounds,
    #[error("free block 범위가 올바르지 않습니다.")]
    InvalidFreeBlockBounds,
    #[error("free start offset이 범위를 초과했습니다.")]
    FreeStartOverflow,
    #[error("slot offset이 page 경계를 벗어났습니다.")]
    SlotOffsetOutOfBounds,
}
