// Implement the SectorsManager 

use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio::fs::{read, rename, remove_file, File};

use crate::{SectorIdx, SectorVec, SectorsManager};

pub struct SectorsManagerState {
    path: PathBuf,
}

impl SectorsManagerState {
    pub async fn new(
        path: PathBuf,
    ) -> Self {
        SectorsManagerState {
            path,
        }
    }
}

#[async_trait::async_trait]
impl SectorsManager for SectorsManagerState {
    async fn read_data(&self, idx: SectorIdx) -> SectorVec {
        unimplemented!()
    }

    async fn read_metadata(&self, idx: SectorIdx) -> (u64, u8) {
        unimplemented!()
    }

    async fn write(&self, idx: SectorIdx, sector: &(SectorVec, u64, u8)) {
        unimplemented!()
    }
}
