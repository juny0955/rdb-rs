use std::cmp::Ordering;

use crate::catalog::metadata::{DataType, SchemaError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeKeyType {
    Int,
    BigInt,
    Boolean,
    Varchar,
}

impl TryFrom<DataType> for BTreeKeyType {
    type Error = SchemaError;

    fn try_from(value: DataType) -> Result<Self, Self::Error> {
        match value {
            DataType::Int => Ok(Self::Int),
            DataType::BigInt => Ok(Self::BigInt),
            DataType::Boolean => Ok(Self::Boolean),
            DataType::Varchar => Ok(Self::Varchar),
            DataType::Null => Err(SchemaError::UnsupportedIndexDataType(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BTreeKey {
    Int(i32),
    BigInt(i64),
    Boolean(bool),
    Varchar(String),
}

impl BTreeKey {
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (BTreeKey::Int(a), BTreeKey::Int(b)) => a.partial_cmp(b),
            (BTreeKey::BigInt(a), BTreeKey::BigInt(b)) => a.partial_cmp(b),
            (BTreeKey::Boolean(a), BTreeKey::Boolean(b)) => a.partial_cmp(b),
            (BTreeKey::Varchar(a), BTreeKey::Varchar(b)) => a.partial_cmp(b),
            _ => None,
        }
    }

    pub fn key_type(&self) -> BTreeKeyType {
        match self {
            BTreeKey::Int(_) => BTreeKeyType::Int,
            BTreeKey::BigInt(_) => BTreeKeyType::BigInt,
            BTreeKey::Boolean(_) => BTreeKeyType::Boolean,
            BTreeKey::Varchar(_) => BTreeKeyType::Varchar,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            BTreeKey::Int(v) => v.to_be_bytes().to_vec(),
            BTreeKey::BigInt(v) => v.to_be_bytes().to_vec(),
            BTreeKey::Boolean(v) => {
                if *v {
                    vec![u8::from_be(1)]
                } else {
                    vec![u8::from_be(0)]
                }
            }
            BTreeKey::Varchar(v) => v.as_bytes().to_vec(),
        }
    }

    pub fn from_bytes(key_type: BTreeKeyType, bytes: &[u8]) -> Option<Self> {
        Some(match key_type {
            BTreeKeyType::Int => BTreeKey::Int(i32::from_be_bytes(bytes.try_into().ok()?)),
            BTreeKeyType::BigInt => BTreeKey::BigInt(i64::from_be_bytes(bytes.try_into().ok()?)),
            BTreeKeyType::Boolean => match bytes {
                [0] => BTreeKey::Boolean(false),
                [1] => BTreeKey::Boolean(true),
                _ => return None,
            },
            BTreeKeyType::Varchar => {
                let Ok(value) = String::from_utf8(bytes.to_vec()) else {
                    return None;
                };

                BTreeKey::Varchar(value)
            }
        })
    }
}
