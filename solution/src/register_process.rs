// Handling messages from processes and from myself.

use std::sync::Arc;
use std::collections::HashMap;
use std::io::Error;

use async_channel::{Sender, Receiver, unbounded};
use hmac::{Mac, Hmac};
use sha2::Sha256;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

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

        RegisterProcessState {
            hmac_system_key,
            hmac_client_key,
            n_sectors,
            sectors_txs,
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
        let (stream, _) = listener.accept().await.expect("Failed to accept connection");
        let rp = register_process.clone();
        tokio::spawn( async move {
            let (mut reader, writer) = stream.into_split();
            let writer_access = Arc::new(Mutex::new(writer));

            let (cmd, hmac_ok) = deserialize_register_command(&mut reader, &rp.hmac_system_key, &rp.hmac_client_key)
            .await.expect("Failed to deserialize command");

            if hmac_ok {
                let sector_idx = match &cmd {
                    RegisterCommand::Client(ClientRegisterCommand { header, .. }) => Some(header.sector_idx),
                    RegisterCommand::System(SystemRegisterCommand { .. }) => None,
                };
                // Valid Hmac and sector index, parse the message and get the callback, to later send the response.
                if sector_idx < Some(rp.n_sectors) {
                    let worker_tx = rp.sectors_txs.get(sector_idx.as_ref().unwrap()).expect("Sector handler not found");

                   let callback = get_success_callback(cmd.clone(), rp.clone(), writer_access.clone());

                    worker_tx.send((cmd, Some(callback))).await.expect("Failed to send message to sector");
                }
                // Receiver message with invalid sector index
                else {
                    // Invalid sector index, send response with InvalidSectorIndex status
                    let (rid, msg_type) = get_rid_and_msg_type(cmd);

                    let resp = construct_invalid_sector_index_response(&rp.hmac_client_key, msg_type.unwrap(), rid.unwrap())
                    .await.expect("Failed to construct response");

                    send_response(resp, writer_access).await;
                }
            }
            // Receiver message with invalid HMAC
            else {
                // Invalid HMac, send response with AuthFailure status
                let (rid, msg_type) = get_rid_and_msg_type(cmd);

                let resp = construct_invalid_hmac_response(&rp.hmac_client_key, msg_type.unwrap(), rid.unwrap())
                .await.expect("Failed to construct response");

                send_response(resp, writer_access).await;
            }
        });
    }
}

// What happens when a message is completed successfuly.
pub fn get_success_callback(
    cmd: RegisterCommand,
    register_process: Arc<RegisterProcessState>,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) -> Callback {
    Box::new(move |op_success: OperationSuccess| Box::pin(async move {
        let (rid, msg_type) = get_rid_and_msg_type(cmd.clone());

        let response = construct_valid_response(
            op_success, &register_process.hmac_client_key, msg_type.unwrap(), rid.unwrap()).await;
        let _ = send_response(response.unwrap(), writer).await;
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

pub fn get_rid_and_msg_type(cmd: RegisterCommand) -> (Option<u64>, Option<u8>) {
    match cmd {
        RegisterCommand::Client(ClientRegisterCommand { header, content }) => {
            match content {
                ClientRegisterCommandContent::Read => (Some(header.request_identifier), Some(0x01)),
                ClientRegisterCommandContent::Write { .. } => (Some(header.request_identifier), Some(0x02)),
            }
        },
        RegisterCommand::System(..) => (None, None),
    }
}
