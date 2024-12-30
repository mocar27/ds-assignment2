// (N,N)-AtomicRegister algorithm implementation as presented in the task description.
// When implementing AtomicRegister, you can assume that RegisterClient 
// passed to the function implements StubbornLink required by the algorithm.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use uuid::Uuid;

use crate::AtomicRegister;
use crate::{RegisterClient, Broadcast as RegisterClientBroadcast, Send as RegisterClientSend} ;
use crate::{SectorIdx, SectorVec, SectorsManager,
    OperationReturn, OperationSuccess, ReadReturn,
    ClientRegisterCommand, ClientRegisterCommandContent, 
    SystemRegisterCommand, SystemCommandHeader, SystemRegisterCommandContent};

pub struct AtomicRegisterState {
    // Identifiers of AtomicRegister are numbered starting at 1 (up to the number of processes in the system).
    self_ident: u8,

    // Identifies of the sector that the AtomicRegister is responsible for.
    sector_idx: SectorIdx,

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

    // Request identifier needed by the algorithm to later return it alongside the callback. 
    rid: Option<u64>,

    // Operation identifier needed by the algorithm, generated in given places, later checked for identity.
    op_id: Option<Uuid>,
}

impl AtomicRegisterState {
    pub async fn new(
        self_ident: u8,
        sector_idx: SectorIdx,
        register_client: Arc<dyn RegisterClient>,
        sectors_manager: Arc<dyn SectorsManager>,
        processes_count: u8,
        ts: u64,
        wr: u8,
        val: SectorVec,
    ) -> Self {
        sectors_manager.write(sector_idx, &(val.clone(), ts.clone(), wr.clone())).await;

        AtomicRegisterState {
            self_ident,
            sector_idx,
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
            writeval: SectorVec(vec![0u8; 4096]),
            readval: SectorVec(vec![0u8; 4096]),
            write_phase: false,
            callback: None,
            rid: None,
            op_id: None,
        }
    }
}

#[async_trait::async_trait]
impl AtomicRegister for AtomicRegisterState {
    // Everything that is going on in this implementation of AtomicRegister for AtomicRegisterState
    // is based on the (N,N)-AtomicRegister algorithm which description was provided in the task
    // description.
    
    // I've provided additional description from the task description in the file 
    // that has been created in the main directory - '(N,N)-AtomicRegister.txt'.

    async fn client_command(
        &mut self,
        cmd: ClientRegisterCommand,
        success_callback: Box<
            dyn FnOnce(OperationSuccess) -> Pin<Box<dyn Future<Output = ()> + Send>>
                + Send
                + Sync,
        >,
    ) {
        // Generated op_id and callback to be called later after the client command is completed.
        let o_id = Uuid::new_v4();
        self.op_id = Some(o_id.clone());
        self.callback = Some(success_callback);
        self.rid = Some(cmd.header.request_identifier);

        match cmd.content {
            // Read client message handling
            ClientRegisterCommandContent::Read => {
                self.readlist.clear();
                self.acklist.clear();
                self.reading = true;
            },

            // Write client message handling
            ClientRegisterCommandContent::Write { data } => {
                self.writeval = data;
                self.acklist.clear();
                self.readlist.clear();
                self.writing = true;
            },
        }
        
        let sidx = cmd.header.sector_idx;

        // Send the ReadProc system message to all processes in the system.
        // Message to be sent construction
        let header = SystemCommandHeader {
            process_identifier: self.self_ident,
            msg_ident: o_id,
            sector_idx: sidx,
        };
        let content = SystemRegisterCommandContent::ReadProc;

        let system_cmd = SystemRegisterCommand {
            header,
            content,
        };

        self.register_client.broadcast( RegisterClientBroadcast { cmd: system_cmd.into() }).await;
    }

    async fn system_command(
        &mut self, 
        cmd: SystemRegisterCommand
    ) {
        // Process each type of the system messages.
        match cmd.content {
            // ReadProc system message handling
            SystemRegisterCommandContent::ReadProc => {
                let p = cmd.header.process_identifier;
                let id = cmd.header.msg_ident;
                let sidx = cmd.header.sector_idx;

                // Message to be sent construction
                let header = SystemCommandHeader {
                    process_identifier: self.self_ident,
                    msg_ident: id,
                    sector_idx: sidx,
                };
                let content = SystemRegisterCommandContent::Value {
                    timestamp: self.ts,
                    write_rank: self.wr,
                    sector_data: self.val.clone(),
                };
                let system_cmd = SystemRegisterCommand {
                    header,
                    content,
                };

                self.register_client.send(RegisterClientSend { cmd: system_cmd.into(), target: p }).await;
            },

            // Value system message handling
            SystemRegisterCommandContent::Value { timestamp, write_rank, sector_data } => {
                let q = cmd.header.process_identifier;
                let id = cmd.header.msg_ident;
                let sidx = cmd.header.sector_idx;

                if id == self.op_id.unwrap() && !self.write_phase {
                    self.readlist.insert(q, (timestamp, write_rank, sector_data));

                    if (self.readlist.len() > (self.processes_count as usize) / 2) && (self.reading || self.writing) {
                        self.readlist.insert(self.self_ident, (self.ts, self.wr, self.val.clone()));

                        // Highest(*) returns the largest value ordered lexicographically by (timestamp, rank)
                        let (maxts, rr, readval) = self.readlist.iter().max_by(|a, b| 
                            a.1.1.cmp(&b.1.1)).map(|(_, v)| v).unwrap().clone();

                        self.readval = readval.clone();
                        self.readlist.clear();
                        self.acklist.clear();
                        self.write_phase = true;
                        
                        // Message to be sent construction
                        let header = SystemCommandHeader {
                            process_identifier: self.self_ident,
                            msg_ident: self.op_id.unwrap(),
                            sector_idx: sidx,
                        };
                        let content: SystemRegisterCommandContent;

                        if self.reading {
                            content = SystemRegisterCommandContent::WriteProc {
                                timestamp: maxts,
                                write_rank: rr,
                                data_to_write: readval.clone(),
                            };
                        }
                        else {
                            // Rank(*) returns a rank of an instance, which is a static number assigned to an instance.
                            (self.ts, self.wr, self.val) = (maxts + 1, self.self_ident, self.writeval.clone());
                            self.sectors_manager.write(self.sector_idx, &(self.val.clone(), self.ts, self.wr)).await;

                            content = SystemRegisterCommandContent::WriteProc {
                                timestamp: self.ts,
                                write_rank: self.self_ident,
                                data_to_write: self.writeval.clone(),
                            };
                        }

                        let system_cmd = SystemRegisterCommand {
                            header,
                            content,
                        };

                        self.register_client.broadcast(RegisterClientBroadcast { cmd: system_cmd.into() }).await;
                    }
                }
            },

            // WriteProc system message handling
            SystemRegisterCommandContent::WriteProc { timestamp, write_rank, data_to_write } => {
                let p = cmd.header.process_identifier;
                let id = cmd.header.msg_ident;
                let sidx = cmd.header.sector_idx;

                if (timestamp, write_rank) > (self.ts, self.wr) {
                    (self.ts, self.wr, self.val) = (timestamp, write_rank, data_to_write.clone());
                    self.sectors_manager.write(self.sector_idx, &(self.val.clone(), self.ts, self.wr)).await;
                }
                
                // Message to be sent construction 
                let header = SystemCommandHeader {
                    process_identifier: self.self_ident,
                    msg_ident: id,
                    sector_idx: sidx,
                };
                let content = SystemRegisterCommandContent::Ack;

                let system_cmd = SystemRegisterCommand {
                    header,
                    content,
                };

                self.register_client.send(RegisterClientSend { cmd: system_cmd.into(), target: p }).await;
            },

            // Ack system message handling
            SystemRegisterCommandContent::Ack => {
                let q = cmd.header.process_identifier;
                let id = cmd.header.msg_ident;

                if id == self.op_id.unwrap() && self.write_phase {
                    self.acklist.insert(q);

                    if (self.acklist.len() > (self.processes_count as usize) / 2) && (self.reading || self.writing) {
                        self.acklist.clear();
                        self.write_phase = false;

                        // After the command is completed, we expect callback to be called.
                        // The request_identifier is the number of request that was provided 
                        // at the beginning by the ClientRegisterCommand header alongside the callback function.
                        // We are constructing the message to be sent and calling the callback function.
                        if self.reading {
                            self.reading = false;
                            
                            let read_return = ReadReturn { read_data: self.readval.clone() };
                            let op_return = OperationReturn::Read(read_return);
                            if let Some(callback) = self.callback.take() {
                                callback(OperationSuccess { request_identifier: self.rid.unwrap(), op_return }).await;
                            }
                        }
                        else {
                            self.writing = false;

                            let op_return = OperationReturn::Write;
                            if let Some(callback) = self.callback.take() {
                                callback(OperationSuccess { request_identifier: self.rid.unwrap(), op_return }).await;
                            }
                        }
                    }
                }
            },
        }
    }
}
