// Atomic register implementation as presented in the task description.

// All its methods shall follow the atomic register algorithm presented above. 
// When implementing AtomicRegister, you can assume that 
// RegisterClient passed to the function implements StubbornLink required by the algorithm.

// Every sector is logically a separate atomic register. 
// However, you should not keep Configuration.public.n_sectors AtomicRegister objects in memory; 
// instead, you should dynamically create and delete them to limit the memory usage (see also the Technical Requirements section).



// – Ensure that every failed write appears either as if it has never been invoked or as if it has completed.
// – A failed read may appear as it has never been invoked

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use tokio::io::{AsyncRead, AsyncWrite};
use std::convert::TryFrom;
use std::io::Error;

use crate::{SectorIdx, SectorVec, RegisterClient, SectorsManager, stable_storage::StableStorage, 
    OperationReturn, OperationSuccess, ReadReturn, StatusCode,
    RegisterCommand, ClientRegisterCommand, ClientCommandHeader, ClientRegisterCommandContent, 
    SystemRegisterCommand, SystemCommandHeader, SystemRegisterCommandContent};


struct AtomicRegisterState {
    self_ident: u8,
    sector_idx: SectorIdx,
    storage: Arc<dyn StableStorage>,
    register_client: Arc<dyn RegisterClient>,
    sectors_manager: Arc<dyn SectorsManager>,
    processes_count: u8,

    ts: u64,
    wr: u64,
    val: Option<Vec<u8>>,

    readlist: HashMap<u8, (u64, u64, Option<SectorVec>)>,
    acklist: HashSet<u8>,

    reading: bool,
    writing: bool,
    writeval: Option<SectorVec>,
    readval: Option<SectorVec>,
    write_phase: bool,

    callback: Option<Box<dyn FnOnce(OperationSuccess) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,
    // op_id: Option<u64>,  // possibly without it, as there is function generate_unique_id() and no stoce(op_id) occurs
}
