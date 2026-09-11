use thiserror::Error;

use crate::{
    catalog::{
        Catalog, CatalogError,
        metadata::{ColumnId, DataType, IndexId, IndexMetadata, RelationId, TableId},
    },
    index::btree::{
        BTreeKey, BTreeKeyType,
        tree::{BTree, BTreeError},
    },
    storage::{
        StorageError, StorageManager,
        page::{PageId, RowId},
    },
    table::{TableError, TableManager},
    tuple::{self, TupleError, Value},
};

pub mod btree;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("인덱스 key로 지원하지 않는 데이터 타입입니다: {0:?}")]
    UnsupportedKeyDataType(DataType),
    #[error("인덱스 엔트리를 찾을 수 없습니다: index={index_id:?}, row={row_id:?}")]
    EntryNotFound { index_id: IndexId, row_id: RowId },
    #[error(transparent)]
    BTree(#[from] BTreeError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Table(#[from] TableError),
    #[error(transparent)]
    Tuple(#[from] TupleError),
}

pub struct IndexManager;
impl IndexManager {
    pub fn create_index(
        storage_manager: &mut StorageManager,
        catalog: &mut Catalog,
        table_manager: &mut TableManager,
        index_name: &str,
        table_id: TableId,
        column_id: ColumnId,
    ) -> Result<(), IndexError> {
        let index_id = catalog.next_index_id(index_name)?;
        let data_type = catalog.index_column_data_type(table_id, column_id)?;
        let root_page_id = Self::create_index_btree(storage_manager, index_id, data_type)?;

        let index_metadata = IndexMetadata::new(
            index_id,
            index_name.to_string(),
            table_id,
            column_id,
            root_page_id,
        );

        Self::backfill(storage_manager, catalog, table_manager, &index_metadata)?;

        catalog.add_index(index_metadata)?;

        Ok(())
    }

    fn create_index_btree(
        storage_manager: &mut StorageManager,
        index_id: IndexId,
        data_type: DataType,
    ) -> Result<PageId, IndexError> {
        let key_type = BTreeKeyType::try_from(data_type)
            .map_err(|_| IndexError::UnsupportedKeyDataType(data_type))?;

        storage_manager.create_relation(RelationId::Index(index_id))?;
        let btree = BTree::create(index_id, key_type, storage_manager)?;
        Ok(btree.root_page_id())
    }

    pub fn insert_row(
        storage_manager: &mut StorageManager,
        catalog: &Catalog,
        table_id: TableId,
        row_id: RowId,
        values: &[Value],
    ) -> Result<(), IndexError> {
        let table = catalog.index_table(table_id)?;

        let indexes = catalog
            .metadata()
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table.id());

        for index_metadata in indexes {
            let column_index =
                catalog.index_column_position(table_id, index_metadata.column_id())?;
            let value = &values[column_index];
            Self::insert_entry(storage_manager, index_metadata, value, row_id)?;
        }

        Ok(())
    }

    fn insert_entry(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        value: &Value,
        row_id: RowId,
    ) -> Result<(), IndexError> {
        if let Some(key) = value_to_btree_key(value) {
            let btree = Self::open_index_btree(
                storage_manager,
                index_metadata.id(),
                index_metadata.root_page_id(),
                key.key_type(),
            )?;

            btree.insert(storage_manager, key, row_id)?;
        }

        Ok(())
    }

    pub fn delete_row(
        storage_manager: &mut StorageManager,
        catalog: &Catalog,
        table_id: TableId,
        row_id: RowId,
        values: &[Value],
    ) -> Result<(), IndexError> {
        let table = catalog.index_table(table_id)?;

        let indexes = catalog
            .metadata()
            .indexes()
            .iter()
            .filter(|index| index.table_id() == table.id());

        for index_metadata in indexes {
            let column_index =
                catalog.index_column_position(table_id, index_metadata.column_id())?;
            let value = &values[column_index];

            Self::delete_entry(storage_manager, index_metadata, value, row_id)?;
        }

        Ok(())
    }

    fn delete_entry(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        value: &Value,
        row_id: RowId,
    ) -> Result<(), IndexError> {
        if let Some(key) = value_to_btree_key(value) {
            let btree = Self::open_index_btree(
                storage_manager,
                index_metadata.id(),
                index_metadata.root_page_id(),
                key.key_type(),
            )?;

            if btree.delete(storage_manager, key, row_id)? {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        Err(IndexError::EntryNotFound {
            index_id: index_metadata.id(),
            row_id,
        })
    }

    pub fn search_index_row_ids(
        storage_manager: &mut StorageManager,
        indexes: &[IndexMetadata],
        table_id: TableId,
        column_id: ColumnId,
        value: &Value,
    ) -> Result<Option<Vec<RowId>>, IndexError> {
        if let Some(index) = indexes
            .iter()
            .find(|index| index.table_id() == table_id && index.column_id() == column_id)
        {
            let row_ids = Self::search_row_ids(storage_manager, index, value)?;
            return Ok(Some(row_ids));
        }

        Ok(None)
    }

    pub fn search_row_ids(
        storage_manager: &mut StorageManager,
        index_metadata: &IndexMetadata,
        value: &Value,
    ) -> Result<Vec<RowId>, IndexError> {
        if let Some(key) = value_to_btree_key(value) {
            let btree = Self::open_index_btree(
                storage_manager,
                index_metadata.id(),
                index_metadata.root_page_id(),
                key.key_type(),
            )?;

            return Ok(btree.search(storage_manager, key)?);
        }

        Ok(vec![])
    }

    fn backfill(
        storage_manager: &mut StorageManager,
        catalog: &Catalog,
        table_manager: &mut TableManager,
        index_metadata: &IndexMetadata,
    ) -> Result<(), IndexError> {
        let rows = table_manager.scan_rows(storage_manager, index_metadata.table_id())?;
        let table = catalog.index_table(index_metadata.table_id())?;
        let column_index = catalog.index_column_position(table.id(), index_metadata.column_id())?;

        let mut entries = Vec::new();
        for (row_id, row) in rows {
            let values = tuple::decode(&row, table.columns())?;
            entries.push((values[column_index].clone(), row_id));
        }

        for (value, row_id) in entries {
            Self::insert_entry(storage_manager, index_metadata, &value, row_id)?;
        }

        Ok(())
    }

    fn open_index_btree(
        storage_manager: &mut StorageManager,
        index_id: IndexId,
        root_page_id: PageId,
        key_type: BTreeKeyType,
    ) -> Result<BTree, IndexError> {
        storage_manager.register_relation(RelationId::Index(index_id))?;
        Ok(BTree::open(index_id, root_page_id, key_type))
    }
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
