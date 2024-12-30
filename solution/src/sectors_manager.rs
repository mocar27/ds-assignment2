// Implement the SectorsManager for sectors storage purposes.
// Local filesystem stores data in blocks of 4096 bytes, the same size as the sectors.
// Solution is allowed to use at most 10% more filesystem space than the size of sectors, which have been written to. 
// That is, if there were writes to n distinct sectors, it is expected that the 
// total directory size does not exceed 1.1 * n * 4096 + constant bytes (with the constant being reasonable). 

use std::path::PathBuf;
use std::sync::Mutex;
use std::collections::HashMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs::{create_dir_all, read_dir, rename, remove_file, File, ReadDir};

use crate::{SectorIdx, SectorVec, SectorsManager};

pub struct SectorsManagerState {
    // We can create files/subdirectories only and exclusively within this directory. 
    path: PathBuf,

    // HashMap to ensure exclusive access to the sector data and metadata.
    data_access: Mutex<HashMap<SectorIdx, (u64, u8)>>
}

impl SectorsManagerState {
    pub async fn new(
        path: PathBuf,
    ) -> Self {
        // Check whether the directory even exists (if not create it) and possibly recover after crash.
        let dir = match read_dir(&path).await {
            Ok(dir) => dir,
            Err(_) => {
                tokio::fs::create_dir_all(&path).await.expect("Failed to create storage directory");
                read_dir(&path).await.expect("Failed to read storage directory")
            }
        };

        let data_access = Self::sectors_crash_recovery(&path, dir).await;

        SectorsManagerState {
            path,
            data_access: Mutex::new(data_access)
        }
    }

    fn sectoridx_file_path(&self, idx: SectorIdx, ts: u64, wr: u8) -> PathBuf {
        self.path.join(format!("{}_{}_{}", idx, ts, wr))
    }

    async fn sectors_crash_recovery(path: &PathBuf, mut dir: ReadDir) -> HashMap<SectorIdx, (u64, u8)> {
        // In case, that we are not creating from zero the storage directory,
        // which basically means that we are recovering from the previous state.
        // We will iterate over the files in the directory and recover all of the data
        // and metadata that were stored in this directory before the crash occured.
        let mut data_access = HashMap::new();

        // Iterate through all entries in the directory and get the important ones (not tmp).
        while let Some(entry) = dir.next_entry().await.expect("Failed to read storage directory") {
            let file_name = entry.file_name();
            let file_name = file_name.to_str().expect("Failed to convert file name to string");
            let parts: Vec<&str> = file_name.split('_').collect();

            // If there are error/tmp/any other files, remove them.
            if parts.len() != 3 {
                remove_file(entry.path()).await.expect("Failed to remove invalid file");
            }
            else {
                let sidx = parts[0].parse::<SectorIdx>().expect("Failed to parse sector index");
                let ts = parts[1].parse::<u64>().expect("Failed to parse timestamp");
                let wr = parts[2].parse::<u8>().expect("Failed to parse write rank");

                // Crash may occure before deleting old file, so we ensure that we are not inluding old file. 
                let (old_ts, old_wr) = data_access.get(&sidx).cloned().unwrap_or((0, 0));
                if (old_ts, old_wr) == (0, 0) {
                    data_access.insert(sidx, (ts, wr));
                }
                else if (ts, wr) > (old_ts, old_wr) {
                    let old_file_path = path.join(format!("{}_{}_{}", sidx, old_ts, old_wr));
                    remove_file(old_file_path).await.expect("Failed to remove old sector file");
                    data_access.insert(sidx, (ts, wr));
                }
                else {
                    remove_file(entry.path()).await.expect("Failed to remove invalid file");
                }
            }
        }
        data_access
    }
}

#[async_trait::async_trait]
impl SectorsManager for SectorsManagerState {
    async fn read_data(&self, idx: SectorIdx) -> SectorVec {
        // Search for the latest inserted timestamp and write rank
        // for the given SectorIdx. 
        // If the data was never inserted, the file_path will not exist
        // and so we will return the default value of SectorVec.
        // Using lock to ensure exclusive access to the HashMap.
        let file_path = {
            let access = self.data_access.lock().expect("Failed to lock data access");
            if let Some(&(ts, wr)) = access.get(&idx) {
                Some(self.sectoridx_file_path(idx, ts, wr))
            } else {
                None
            }
        };

        // If the file with data exists for given SectorIdx, return the data
        // else we will return the default value of the SectorVec.
        match file_path {
            Some(file_path) => {
                let mut file = File::open(file_path).await.expect("Failed to open sector file");
                let mut data = vec![0u8; 4096];
                file.read_exact(&mut data).await.expect("Failed to read sector data");
                SectorVec(data)
            },
            None => SectorVec(vec![0u8; 4096])
        }
    }

    async fn read_metadata(&self, idx: SectorIdx) -> (u64, u8) {
        // Recover metadata from the HashMap.
        // Lock for the exclusive access, so nobody overrides the data.
        let access = self.data_access.lock().expect("Failed to lock data access");
        *access.get(&idx).unwrap_or(&(0, 0))
    }

    async fn write(&self, idx: SectorIdx, sector: &(SectorVec, u64, u8)) {
        // Hint: to fulfill the above requirement, you can store sector metadata in filenames. 
        // The recovery of SectorsManager can have O(n) time complexity.
        // Scheme similar to the one presented in the dslab06.

        // Setup new data to be written for the SectorIdx.
        let (data, ts, wr) = sector;
        let file_path = self.sectoridx_file_path(idx, *ts, *wr);
        let tmp_file_path = self.path.join("tmp_").join(&file_path);

        // Write all the data to the temporary file.
        let mut tmp_file = File::create(&tmp_file_path).await.expect("Failed to create temporary sector file");
        tmp_file.write_all(&data.0).await.expect("Failed to write sector data to temporary file");
        tmp_file.sync_data().await.expect("Failed to sync temporary sector file");

        // Create file with new metadata and data inside the file and save it.
        // The temporary file will be remaned to the destination file, 
        // so we do not use additional memory. After that perform a sync on the data.
        rename(tmp_file_path, &file_path).await.expect("Failed to rename temporary sector file");
        let dest = File::open(&file_path).await.expect("Failed to open sector file");
        dest.sync_data().await.expect("Failed to sync sector file");

        // Recover old metadata from the HashMap to delete the old data later.
        let (old_ts, old_wr) = {
            let access = self.data_access.lock().expect("Failed to lock data access");
            access.get(&idx).cloned().unwrap_or((0, 0))
        };

        // At this point the file with new data and old data (if it existed) are both saved.
        // Now we can update metadata that new data is saved for the SectorIdx.
        {
            let mut access = self.data_access.lock().expect("Failed to lock data access");
            access.insert(idx, (*ts, *wr));
        }

        // Knowing we have new data and metadata saved for given SectorIdx,
        // we can remove the old data safely.
        if (old_ts, old_wr) != (0, 0) {
            let old_file_path = self.sectoridx_file_path(idx, old_ts, old_wr);
            remove_file(&old_file_path).await.expect("Failed to remove old sector file");
        }
    }
}
