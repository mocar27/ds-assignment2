mod domain;
mod atomic_register;
mod register_client;
mod register_process;
mod sectors_manager;
mod transfer_operations;
mod utils;

use crate::register_process::{RegisterProcessState, handle_self_messages, accept_connections};
use crate::register_client::RegisterClientState;
use tokio::net::TcpListener;
use async_channel::unbounded;
use std::sync::Arc;

pub use crate::domain::*;
pub use atomic_register_public::*;
pub use register_client_public::*;
pub use sectors_manager_public::*;
pub use transfer_public::*;

pub async fn run_register_process(config: Configuration) {
    let hmac_system_key = config.hmac_system_key;
    let hmac_client_key = config.hmac_client_key;
    let storage_dir = config.public.storage_dir.clone();
    let tcp_locations = config.public.tcp_locations.clone();
    let self_rank = config.public.self_rank;
    let n_sectors = config.public.n_sectors;

    // Bind the listener to the address first, as told to in the task.
    let addr = tcp_locations.get(self_rank as usize - 1)
        .expect("Self rank is out of tcp_locations vector bounds")
        .clone();
    let listener = TcpListener::bind(addr.clone()).await.expect("Failed to bind to address");

    // Channel for sending messages to self, so we are skipping serialization, deserialization, TCP, etc.
    let (self_tx, self_rx) = unbounded::<SystemRegisterCommand>();
    
    let register_client = Arc::new(RegisterClientState::new(
        self_rank.clone(),
        hmac_system_key.clone(),
        self_tx,
        tcp_locations.len() as u8,
        tcp_locations.clone()
    ).await);
        
    // Create sectors manager that will operate on that directory.
    let sectors_manager = build_sectors_manager(storage_dir.clone()).await;

    let register_process = Arc::new(RegisterProcessState::new(
        self_rank.clone(),
        hmac_system_key.clone(),
        hmac_client_key.clone(),
        tcp_locations.len() as u8,
        n_sectors.clone(),
        register_client.clone(),
        sectors_manager.clone(),
    ).await);

    let self_handler = tokio::spawn(handle_self_messages(self_rx, register_process.clone()));

    let connections_handler = tokio::spawn(accept_connections(
        listener,
        register_process.clone(),
    ));

    connections_handler.await.expect("Connections handler failed");
    self_handler.await.expect("Self handler failed");
}

pub mod atomic_register_public {
    use crate::{
        ClientRegisterCommand, OperationSuccess, RegisterClient, SectorIdx, SectorsManager,
        SystemRegisterCommand,
    };
    use crate::atomic_register::AtomicRegisterState;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    #[async_trait::async_trait]
    pub trait AtomicRegister: Send + Sync {
        /// Handle a client command. After the command is completed, we expect
        /// callback to be called. Note that completion of client command happens after
        /// delivery of multiple system commands to the register, as the algorithm specifies.
        ///
        /// This function corresponds to the handlers of Read and Write events in the
        /// (N,N)-AtomicRegister algorithm.
        async fn client_command(
            &mut self,
            cmd: ClientRegisterCommand,
            success_callback: Box<
                dyn FnOnce(OperationSuccess) -> Pin<Box<dyn Future<Output = ()> + Send>>
                    + Send
                    + Sync,
            >,
        );

        /// Handle a system command.
        ///
        /// This function corresponds to the handlers of READ_PROC, VALUE, WRITE_PROC
        /// and ACK messages in the (N,N)-AtomicRegister algorithm.
        async fn system_command(&mut self, cmd: SystemRegisterCommand);
    }

    /// Idents are numbered starting at 1 (up to the number of processes in the system).
    /// Communication with other processes of the system is to be done by register_client.
    /// And sectors must be stored in the sectors_manager instance.
    ///
    /// This function corresponds to the handlers of Init and Recovery events in the
    /// (N,N)-AtomicRegister algorithm.
    pub async fn build_atomic_register(
        self_ident: u8,
        sector_idx: SectorIdx,
        register_client: Arc<dyn RegisterClient>,
        sectors_manager: Arc<dyn SectorsManager>,
        processes_count: u8,
    ) -> Box<dyn AtomicRegister> {
        // Recover data and metadata from sectors_manager
        // to handle the recovery event in the (N,N)-AtomicRegister algorithm

        // If the data was never written to the sector, then these functions will return
        // default values for the data and metadata, which are zero-th vector of size 4096 for val
        // and (0, 0) for ts and wr (sectors_manager implementation problem).
        let (ts, wr) = sectors_manager.read_metadata(sector_idx).await;
        let val = sectors_manager.read_data(sector_idx).await;
        
        Box::new(AtomicRegisterState::new(
            self_ident, 
            sector_idx,
            register_client, 
            sectors_manager, 
            processes_count, 
            ts, 
            wr, 
            val
        ).await)
    }
}

pub mod sectors_manager_public {
    use crate::{SectorIdx, SectorVec};
    use crate::sectors_manager::SectorsManagerState;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[async_trait::async_trait]
    pub trait SectorsManager: Send + Sync {
        /// Returns 4096 bytes of sector data by index.
        async fn read_data(&self, idx: SectorIdx) -> SectorVec;

        /// Returns timestamp and write rank of the process which has saved this data.
        /// Timestamps and ranks are relevant for atomic register algorithm, and are described
        /// there.
        async fn read_metadata(&self, idx: SectorIdx) -> (u64, u8);

        /// Writes a new data, along with timestamp and write rank to some sector.
        async fn write(&self, idx: SectorIdx, sector: &(SectorVec, u64, u8));
    }

    /// Path parameter points to a directory to which this method has exclusive access.
    pub async fn build_sectors_manager(path: PathBuf) -> Arc<dyn SectorsManager> {
        Arc::new(SectorsManagerState::new(
            path
        ).await)
    }
}

pub mod transfer_public {
    use crate::transfer_operations::{deserialize_rc, serialize_rc};
    use crate::RegisterCommand;
    use std::io::Error;
    use tokio::io::{AsyncRead, AsyncWrite};

    pub async fn deserialize_register_command(
        data: &mut (dyn AsyncRead + Send + Unpin),
        hmac_system_key: &[u8; 64],
        hmac_client_key: &[u8; 32],
    ) -> Result<(RegisterCommand, bool), Error> {
        deserialize_rc(data, hmac_system_key, hmac_client_key).await
    }

    pub async fn serialize_register_command(
        cmd: &RegisterCommand,
        writer: &mut (dyn AsyncWrite + Send + Unpin),
        hmac_key: &[u8],
    ) -> Result<(), Error> {
        serialize_rc(cmd, writer, hmac_key).await
    }
}

pub mod register_client_public {
    use crate::SystemRegisterCommand;
    use std::sync::Arc;

    #[async_trait::async_trait]
    /// We do not need any public implementation of this trait. It is there for use
    /// in AtomicRegister. In our opinion it is a safe bet to say some structure of
    /// this kind must appear in your solution.
    pub trait RegisterClient: core::marker::Send + core::marker::Sync {
        /// Sends a system message to a single process.
        async fn send(&self, msg: Send);

        /// Broadcasts a system message to all processes in the system, including self.
        async fn broadcast(&self, msg: Broadcast);
    }

    pub struct Broadcast {
        pub cmd: Arc<SystemRegisterCommand>,
    }

    pub struct Send {
        pub cmd: Arc<SystemRegisterCommand>,
        /// Identifier of the target process. Those start at 1.
        pub target: u8,
    }
}
