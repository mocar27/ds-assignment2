// (N,N)-AtomicRegister algorithm implementation as presented in the task description.
// When implementing AtomicRegister, you can assume that RegisterClient 
// passed to the function implements StubbornLink required by the algorithm.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use uuid::Uuid;
use tokio::io::{AsyncRead, AsyncWrite};
use std::convert::TryFrom;
use std::io::Error;

use crate::AtomicRegister;
use crate::{SectorIdx, SectorVec, RegisterClient, SectorsManager, stable_storage::StableStorage, 
    OperationReturn, OperationSuccess, ReadReturn, StatusCode,
    RegisterCommand, ClientRegisterCommand, ClientCommandHeader, ClientRegisterCommandContent, 
    SystemRegisterCommand, SystemCommandHeader, SystemRegisterCommandContent};

pub struct AtomicRegisterState {
    // Identifiers of AtomicRegister are numbered starting at 1 (up to the number of processes in the system).
    self_ident: u8,

    // RegisterClient is used to communicate with other processes of the system.
    register_client: Arc<dyn RegisterClient>,

    // SectorsManager is used to perform read and write operations on sectors (StableStorage inside of SectorsManager).
    sectors_manager: Arc<dyn SectorsManager>,

    // The number of processes in the system
    processes_count: u8,

    // Timestamp, Write rank, Value as described in the algorithm.
    ts: u64,
    wr: u8,
    val: SectorVec,

    // Map of reading by processes, with the (timestamp, write rank, value) respectively.
    readlist: HashMap<u8, (u64, u8, SectorVec)>,
    
    // Set of processes that have accepted the write request (Write-Consult-Majority). 
    acklist: HashSet<u8>,
    
    // Fields specified by the algorithm description for the AtomicRegister.
    reading: bool,
    writing: bool,
    writeval: SectorVec,
    readval: SectorVec,
    write_phase: bool,

    // After the client command is completed, we expect callback to be called.
    callback: Option<Box<dyn FnOnce(OperationSuccess) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>>,

    // Operation identifier needed by the algorithm, generated in given places, later checked for identity.
    op_id: Option<Uuid>,
}

impl AtomicRegisterState {
    // 1.
    // The rank(*) returns a rank of an instance, which is a static number assigned to an instance. 
    // The highest(*) returns the largest value ordered lexicographically by (timestamp, rank).

    // 2.
    // The atomic register enforces constraints between events on processes, 
    // and thereby it makes it possible to put all read and write operations on a single timeline, 
    // and to mark the start and end of each operation.

    // 2'.
    // Every read returns the most recently written value. 
    // If an operation o happens before operation o' 
    // when the system is processing messages, 
    // then o must appear before o' on such a common timeline. 
    // This is called linearization.

    pub async fn new(
        self_ident: u8,
        register_client: Arc<dyn RegisterClient>,
        sectors_manager: Arc<dyn SectorsManager>,
        processes_count: u8,
        ts: u64,
        wr: u8,
        val: SectorVec,
    ) -> Self {
        AtomicRegisterState {
            self_ident,
            register_client,
            sectors_manager,
            processes_count,
            ts,
            wr,
            val,
            readlist: HashMap::new(),
            acklist: HashSet::new(),
            reading: false,
            writing: false,
            writeval: SectorVec(Vec::new()),
            readval: SectorVec(Vec::new()),
            write_phase: false,
            callback: None,
            op_id: None,
        }
    }

}

#[async_trait::async_trait]
impl AtomicRegister for AtomicRegisterState {
    async fn client_command(
        &mut self,
        cmd: ClientRegisterCommand,
        success_callback: Box<
            dyn FnOnce(OperationSuccess) -> Pin<Box<dyn Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    ) {
        unimplemented!()
    }

    async fn system_command(
        &mut self, 
        cmd: SystemRegisterCommand
    ) {
        unimplemented!()
    }
}
