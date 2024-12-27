// Implement the SectorsManager for storage purposes.

// There is a limit on the number of open file descriptors: 1024. 
// We suggest utilizing it for maximum concurrency. 

// There will not be more than 16 client connections.
// Local filesystem stores data in blocks of 4096 bytes, the same size as the sectors.

// Solution is allowed to use at most 10% more filesystem space than the size of sectors, which have been written to. 
// That is, if there were writes to n distinct sectors, it is expected that the 
// total directory size does not exceed 1.1 * n * 4096 + constant bytes (with the constant being reasonable). 

// Temporary files used for ensuring atomicity do not count towards this limit. 
// However, they must be removed when they are no longer necessary. 

// When the system is not handling any messages, the filesystem usage should be below the limit.

// Hint: to fulfill the above requirement, you can try storing sector metadata in filenames. 
// The recovery of SectorsManager can have O(n) time complexity.

use std::path::PathBuf;
use std::sync::Mutex;
use std::collections::HashMap;

use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use tokio::fs::{read, rename, remove_file, File};

use crate::{SectorIdx, SectorVec, SectorsManager};

pub struct SectorsManagerState {
    // We can create subdirectories only and exclusively within this directory. 
    path: PathBuf,

    // HashMap to ensure exclusive access to the sector data and metadata.
    // To eliminate concurrent reads and writes of data.
    // Even tho unit test doesn't perform concurrent reads and writes,
    // we need it for further implementation, not only for unit testing.
    sector_access: Mutex<HashMap<SectorIdx, (u64, u8)>>
}

impl SectorsManagerState {
    pub async fn new(
        path: PathBuf,
    ) -> Self {
        SectorsManagerState {
            path,
            sector_access: Mutex::new(HashMap::new())
        }
    }
}

#[async_trait::async_trait]
impl SectorsManager for SectorsManagerState {
    async fn read_data(&self, idx: SectorIdx) -> SectorVec {
        // SectorVec data for the given SectorIdx can be stored INSIDE the file named as the SectorIdx.
        unimplemented!()
    }

    async fn read_metadata(&self, idx: SectorIdx) -> (u64, u8) {
        // Metadata stored in the name of the file with the SectorIdx (with inside data SectorVec).
        unimplemented!()
    }

    async fn write(&self, idx: SectorIdx, sector: &(SectorVec, u64, u8)) {
        unimplemented!()
    }
}
