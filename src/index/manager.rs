use std::path::{Path, PathBuf};

use crate::{
    catalog::metadata::{IndexId, IndexMetadata, RelationId},
    index::btree::{
        BTreeKey, BTreeKeyType,
        tree::{BTree, BTreeError},
    },
    storage::{
        manager::StorageManager,
        page::{PageId, RowId},
    },
    tuple::Value,
};

pub enum DeleteResult {
    Deleted,
    NotFound,
    SkippedNull,
}

pub struct IndexManager;
impl IndexManager {
    pub fn create_index_btree(
        storage_manager: &mut StorageManager,
        index_id: IndexId,
        key_type: BTreeKeyType,
        data_dir: &Path,
    ) -> Result<PageId, BTreeError> {
        storage_manager
            .create_relation(RelationId::Index(index_id), index_path(data_dir, index_id))?;
        let btree = BTree::create(index_id, key_type, storage_manager)?;
        Ok(btree.root_page_id())
    }

    pub fn insert_entry(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        value: &Value,
        data_dir: &Path,
        row_id: RowId,
    ) -> Result<(), BTreeError> {
        if let Some(key) = value_to_btree_key(value) {
            let btree = Self::open_index_btree(
                storage_manager,
                index_metadata.id(),
                index_metadata.root_page_id(),
                key.key_type(),
                data_dir,
            );

            btree.insert(storage_manager, key, row_id)?;
        }

        Ok(())
    }

    pub fn delete_entry(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        value: &Value,
        data_dir: &Path,
        row_id: RowId,
    ) -> Result<DeleteResult, BTreeError> {
        if let Some(key) = value_to_btree_key(value) {
            let btree = Self::open_index_btree(
                storage_manager,
                index_metadata.id(),
                index_metadata.root_page_id(),
                key.key_type(),
                data_dir,
            );

            if btree.delete(storage_manager, key, row_id)? {
                Ok(DeleteResult::Deleted)
            } else {
                Ok(DeleteResult::NotFound)
            }
        } else {
            Ok(DeleteResult::SkippedNull)
        }
    }

    pub fn search_row_ids(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        value: &Value,
        data_dir: &Path,
    ) -> Result<Vec<RowId>, BTreeError> {
        if let Some(key) = value_to_btree_key(value) {
            let btree = Self::open_index_btree(
                storage_manager,
                index_metadata.id(),
                index_metadata.root_page_id(),
                key.key_type(),
                data_dir,
            );

            return btree.search(storage_manager, key);
        }

        Ok(vec![])
    }

    pub fn backfill_entries(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        entries: Vec<(Value, RowId)>,
        data_dir: &Path,
    ) -> Result<(), BTreeError> {
        for (value, row_id) in entries {
            Self::insert_entry(storage_manager, index_metadata, &value, data_dir, row_id)?;
        }

        Ok(())
    }

    fn open_index_btree(
        storage_manager: &mut StorageManager,
        index_id: IndexId,
        root_page_id: PageId,
        key_type: BTreeKeyType,
        data_dir: &Path,
    ) -> BTree {
        storage_manager
            .register_relation(RelationId::Index(index_id), index_path(data_dir, index_id));
        BTree::open(index_id, root_page_id, key_type)
    }
}

fn index_path(dir: &Path, index_id: IndexId) -> PathBuf {
    dir.join(format!("{}.idx", index_id.id()))
}

fn value_to_btree_key(value: &Value) -> Option<BTreeKey> {
    match value {
        Value::Int(v) => Some(BTreeKey::Int(*v)),
        Value::BigInt(v) => Some(BTreeKey::BigInt(*v)),
        Value::Boolean(v) => Some(BTreeKey::Boolean(*v)),
        Value::Varchar(v) => Some(BTreeKey::Varchar(v.clone())),
        Value::Null => None,
    }
}
