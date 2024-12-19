// Stable storage inspired by the task assignment on lab06.
use std::path::PathBuf;

use tokio::fs::{read, rename, remove_file, File};
use tokio::io::AsyncWriteExt;

use sha2::{Sha256, Digest};

#[async_trait::async_trait]
pub trait StableStorage: Send + Sync {
    /// Stores `value` under `key`.
    async fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String>;

    /// Retrieves value stored under `key`.
    async fn get(&self, key: &str) -> Option<Vec<u8>>;

    /// Removes `key` and the value stored under it.
    async fn remove(&mut self, key: &str) -> bool;
}

struct StableStorageData {
    root_dir: PathBuf,
}

impl StableStorageData {
    fn validate_key(key: &str) -> Result<(), String> {
        if key.len() > 255 {
            return Err("Key length exceeds 255 bytes".into());
        }
        Ok(())
    }

    fn validate_value(value: &[u8]) -> Result<(), String> {
        if value.len() > 65535 {
            return Err("Value length exceeds 65535 bytes".into());
        }
        Ok(())
    }

    fn hash_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    fn key_path(&self, key: &str) -> PathBuf {
        let hashed_key = Self::hash_key(key);
        self.root_dir.join(hashed_key)
    }
}

#[async_trait::async_trait]
impl StableStorage for StableStorageData { 
    async fn put(&mut self, key: &str, value: &[u8]) -> Result<(), String> {
        unimplemented!()
    }

    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        unimplemented!()
    }

    async fn remove(&mut self, key: &str) -> bool {
        unimplemented!()
    }
}

/// Creates a new instance of stable storage.
pub async fn build_stable_storage(root_storage_dir: PathBuf) -> Box<dyn StableStorage> {
    Box::new(StableStorageData {
        root_dir: root_storage_dir,
    })
}
