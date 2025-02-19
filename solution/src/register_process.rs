// Handling incoming messages from other processes, client and myself.
// The main function is accept_connections, which is called by the main function in lib.rs

use std::sync::Arc;
use std::collections::HashMap;
use std::io::Error;

use async_channel::{Sender, Receiver, unbounded};
use hmac::{Mac, Hmac};
use sha2::Sha256;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

type HmacSha256 = Hmac<Sha256>;

use crate::{build_atomic_register, SectorIdx, StatusCode,
    RegisterCommand, OperationReturn, OperationSuccess, ReadReturn, 
    ClientRegisterCommand, ClientRegisterCommandContent, SystemRegisterCommand,
    RegisterClient, SectorsManager, utils::Callback, MAGIC_NUMBER,
    deserialize_register_command};

#[derive(Debug, Clone)]
pub struct RegisterProcessState {
    hmac_system_key: [u8; 64],
    hmac_client_key: [u8; 32],
    n_sectors: u64,

    sectors_txs: HashMap<SectorIdx, Sender<(RegisterCommand, Option<Callback>)>>,
    sectors_guards: HashMap<SectorIdx, Arc<Semaphore>>,
}

impl RegisterProcessState {
    pub async fn new(
        self_ident: u8,
        hmac_system_key: [u8; 64],
        hmac_client_key: [u8; 32],
        processes_count: u8,
        n_sectors: u64,
        register_client: Arc<dyn RegisterClient>,
        sectors_manager: Arc<dyn SectorsManager>,
    ) -> Self {

        let mut sectors_txs: HashMap<SectorIdx, Sender<(RegisterCommand, Option<Callback>)>> = HashMap::new();
        for i in 0..n_sectors {
            let (tx, rx) = unbounded();
            sectors_txs.insert(i, tx);
            tokio::spawn(run_ar_worker(rx, self_ident, i, register_client.clone(), sectors_manager.clone(), processes_count.clone()));
        }

        let mut sectors_guards: HashMap<SectorIdx, Arc<Semaphore>> = HashMap::new();
        for i in 0..n_sectors {
            sectors_guards.insert(i, Arc::new(Semaphore::new(1)));
        }

        RegisterProcessState {
            hmac_system_key,
            hmac_client_key,
            n_sectors,
            sectors_txs,
            sectors_guards,
        }
    }
}

pub async fn run_ar_worker(
    ar_worker_rx: Receiver<(RegisterCommand, Option<Callback>)>,
    self_ident: u8,
    sector_idx: SectorIdx,
    register_client: Arc<dyn RegisterClient>,
    sectors_manager: Arc<dyn SectorsManager>,
    processes_count: u8,
) {
    let mut ar_worker = build_atomic_register(self_ident, sector_idx, register_client, sectors_manager, processes_count).await;
    loop {
        let (cmd, callback) = ar_worker_rx.recv().await.expect("Failed to receive message");
        match cmd {
            RegisterCommand::Client(c) => {
                ar_worker.client_command(c, callback.unwrap()).await;
            },
            RegisterCommand::System(s) => {
                ar_worker.system_command(s).await;
            }
        }
    }
}

pub async fn handle_self_messages(
    self_rx: Receiver<SystemRegisterCommand>,
    register_process: Arc<RegisterProcessState>,
) {
    while let Ok(msg) = self_rx.recv().await {
        let sidx = msg.header.sector_idx;
        let tx = register_process.sectors_txs.get(&sidx).expect("Sector handler not found");
        let cmd = RegisterCommand::System(msg);
        tx.send((cmd, None)).await.expect("Failed to send message to sector");
    }
}

pub async fn accept_connections(
    listener: tokio::net::TcpListener,
    register_process: Arc<RegisterProcessState>,
) {
    loop {
        // Accept the connection and split the stream into reader and writer.
        let (stream, _) = listener.accept().await.expect("Failed to accept connection");
        stream.set_nodelay(true).expect("Failed to set nodelay");
        let (mut reader, writer) = stream.into_split();
        let writer_access = Arc::new(Mutex::new(writer));
        
        let rp = register_process.clone();

        tokio::spawn( async move {
            loop {
                let (cmd, hmac_ok) = deserialize_register_command(&mut reader, &rp.hmac_system_key, &rp.hmac_client_key)
                .await.expect("Failed to deserialize command");

                if hmac_ok {
                    match cmd {
                        RegisterCommand::Client(ClientRegisterCommand { header, .. }) => {
                            let sector_idx = header.sector_idx;

                            // Valid Hmac and sector index, parse the message and get the callback, to later send the response.
                            if sector_idx < rp.n_sectors {
                                let worker_tx = rp.sectors_txs.get(&sector_idx).expect("Sector handler not found");

                                // However, there can be multiple clients, and different clients can each send a command with the same sector index at the same time.
                                // We need to take cover of that, so we don't get mixed and messed up while processing the message (I got messed up).
                                let sector_guard = rp.sectors_guards.get(&sector_idx).expect("Sector mutex not found");
                                let guard = sector_guard.clone().acquire_owned().await.expect("Failed to acquire semaphore");

                                let callback = success_callback(cmd.clone(), rp.clone(), writer_access.clone(), guard);

                                worker_tx.send((cmd, Some(callback))).await.expect("Failed to send message to sector");
                            }
                            // Receiver message with invalid sector index
                            else {
                                // Invalid sector index, send response with InvalidSectorIndex status
                                let (rid, msg_type) = get_rid_and_msg_type(cmd);

                                let resp = construct_invalid_sector_index_response(&rp.hmac_client_key, msg_type, rid)
                                .await.expect("Failed to construct response");

                                send_response(resp, writer_access.clone()).await;
                            }
                        },
                        RegisterCommand::System(SystemRegisterCommand { header, .. })=> {                            
                            let sidx = header.sector_idx;
                            let tx = rp.sectors_txs.get(&sidx).expect("Sector handler not found");
                            tx.send((cmd, None)).await.expect("Failed to send message to sector");
                        },
                    }
                }
                // Received message with invalid HMAC
                else {
                    if let RegisterCommand::Client(..) = cmd {
                        // Invalid HMac, send response with AuthFailure status
                        let (rid, msg_type) = get_rid_and_msg_type(cmd);

                        let resp = construct_invalid_hmac_response(&rp.hmac_client_key, msg_type, rid)
                        .await.expect("Failed to construct response");

                        send_response(resp, writer_access.clone()).await;
                    }
                    // else if let RegisterCommand::System(SystemRegisterCommand { header, .. }) = cmd {
                    //     if rp.self_ident == 3 {
                    //         println!("I am process {} and I received a command from {} with invalid HMAC", rp.self_ident, header.process_identifier);
                    //     }
                        // println!("I am process {} and I received a command from {} with invalid HMAC", rp.self_ident, header.process_identifier);
                    // }
                }
            }
        });
    }
}

// What happens when a message is completed successfuly.
pub fn success_callback(
    cmd: RegisterCommand,
    register_process: Arc<RegisterProcessState>,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    guard: OwnedSemaphorePermit,
) -> Callback {
    Box::new(move |op_success: OperationSuccess| Box::pin(async move {
        let (rid, msg_type) = get_rid_and_msg_type(cmd.clone());

        let response = construct_valid_response(
            op_success, &register_process.hmac_client_key, msg_type, rid).await;
            
        let _ = send_response(response.unwrap(), writer).await;
        drop(guard);
    }))
}

pub async fn construct_valid_response(
    op_success: OperationSuccess,
    hmac_client_key: &[u8; 32],
    msg_type: u8,
    request_identifier: u64,
) -> Result<Vec<u8>, Error> {
    let mut buf: Vec<u8> = Vec::new();
    let status_code = StatusCode::Ok as u8;
    let response_msg_type = msg_type + 0x40;

    buf.write_all(&MAGIC_NUMBER).await?;
    buf.write_all(&[0, 0, status_code, response_msg_type]).await?;
    buf.write_all(&request_identifier.to_be_bytes()).await?;

    if let OperationReturn::Read(ReadReturn { read_data }) = op_success.op_return {
        let data: Vec<u8> = read_data.0.to_vec();
        buf.write_all(&data).await?;
    }

    let mut mac = HmacSha256::new_from_slice(hmac_client_key).expect("HMAC can take key of any size");
    mac.update(&buf);
    let tag: [u8; 32] = mac.finalize().into_bytes().as_slice().try_into().expect("HMAC output is of the correct length");
    buf.write_all(&tag).await?;

    Ok(buf)
}

pub async fn construct_invalid_hmac_response(
    hmac_client_key: &[u8; 32],
    msg_type: u8,
    request_identifier: u64,
) -> Result<Vec<u8>, Error> {
    let mut buf: Vec<u8> = Vec::new();
    let status_code = StatusCode::AuthFailure as u8;
    let response_msg_type = msg_type + 0x40;

    buf.write_all(&MAGIC_NUMBER).await?;
    buf.write_all(&[0, 0, status_code, response_msg_type]).await?;
    buf.write_all(&request_identifier.to_be_bytes()).await?;

    let mut mac = HmacSha256::new_from_slice(hmac_client_key).expect("HMAC can take key of any size");
    mac.update(&buf);
    let tag: [u8; 32] = mac.finalize().into_bytes().as_slice().try_into().expect("HMAC output is of the correct length");
    buf.write_all(&tag).await?;

    Ok(buf)
}

pub async fn construct_invalid_sector_index_response(
    hmac_client_key: &[u8; 32],
    msg_type: u8,
    request_identifier: u64,
) -> Result<Vec<u8>, Error> {
    let mut buf: Vec<u8> = Vec::new();
    let status_code = StatusCode::InvalidSectorIndex as u8;
    let response_msg_type = msg_type + 0x40;

    buf.write_all(&MAGIC_NUMBER).await?;
    buf.write_all(&[0, 0, status_code, response_msg_type]).await?;
    buf.write_all(&request_identifier.to_be_bytes()).await?;

    let mut mac = HmacSha256::new_from_slice(hmac_client_key).expect("HMAC can take key of any size");
    mac.update(&buf);
    let tag: [u8; 32] = mac.finalize().into_bytes().as_slice().try_into().expect("HMAC output is of the correct length");
    buf.write_all(&tag).await?;

    Ok(buf)
}

pub async fn send_response(
    reponse: Vec<u8>,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) {
    let mut writer = writer.lock().await;
    writer.write_all(&reponse).await.expect("Failed to write response");
}

pub fn get_rid_and_msg_type(cmd: RegisterCommand) -> (u64, u8) {
    match cmd {
        RegisterCommand::Client(ClientRegisterCommand { header, content }) => {
            match content {
                ClientRegisterCommandContent::Read => (header.request_identifier, 0x01),
                ClientRegisterCommandContent::Write { .. } => (header.request_identifier, 0x02),
            }
        },
        RegisterCommand::System(..) => {
            panic!("System command should not be handled here");
        },
    }
}
