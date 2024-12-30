// RegisterClient implementation for TCP communication between processes of the distributed register.

// An instance is passed to instances of AtomicRegister to allow them communicating with each other.
// Every process of the distributed register has multiple copies of the atomic register code,
// which copies share a component that handles communication (RegisterClient).

// Messages sent by a process to itself skip TCP, serialization, deserialization, 
// HMAC preparation and validation phases to improve the performance

// (N,N)-AtomicRegister algorithm relies on stubborn links, 
// which send messages forever (unless the sender crashes).
// This behavior models retransmissions (with 500 ms delay interval). 
// Retransmission will continue until the receiver acknowledges that process has recovered.

use std::collections::HashMap;
use async_channel::{Receiver, Sender, unbounded};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::time::{Duration, Instant, interval_at};

use crate::{RegisterCommand, SystemRegisterCommand,
    RegisterClient, Send, Broadcast, serialize_register_command};

pub static DELAY : Duration = Duration::from_millis(500);

pub struct RegisterClientState {
    // Data needed for broadcasting and creating connections.
    processes_count: u8,

    // Channel for giving the information to task handlers that we want to send a message to some process.
    // Later the task will consume the message on the channel and send it to the target process.
    // This way we don't pass the sending socket to every task handler in the system.
    processes_txs: HashMap<u8, Sender<SystemRegisterCommand>>,

    // Retransmission channel for every process in the system.
    retransmission_txs: HashMap<u8, Sender<SystemRegisterCommand>>,
}

impl RegisterClientState {
    pub async fn new(
        self_ident: u8,
        hmac_system_key: [u8; 64],
        self_tx: Sender<SystemRegisterCommand>,
        processes_count: u8,
        tcp_locations: Vec<(String, u16)>,
    ) -> Self {
        // Map of processes' identifiers to their sending channels (so that connection handlers know when to write).
        let mut processes_txs = HashMap::new();
        let mut retransmission_txs = HashMap::new();
        processes_txs.insert(self_ident.clone(), self_tx.clone());
        retransmission_txs.insert(self_ident.clone(), self_tx.clone());

        // After 300 ms all processes should have called bind and already be listening.
        tokio::time::sleep(Duration::from_millis(300)).await;
        for i in 0..processes_count {
            // Adress of each process is under tcp_locations[i - 1]
            // Create a connection to every other process in the system (except self_ident address).
            if i != self_ident - 1 {
                let addr = &tcp_locations[i as usize];
                let stream = TcpStream::connect(addr).await.expect("Failed to connect to process");
                stream.set_nodelay(true).expect("Failed to set nodelay");

                let (connection_tx, connection_rx) = unbounded::<SystemRegisterCommand>();
                let (retransmission_tx, retransmission_rx) = unbounded::<SystemRegisterCommand>();
                
                let process_ident = i + 1;
                processes_txs.insert(process_ident.clone(), connection_tx.clone());
                retransmission_txs.insert(process_ident.clone(), retransmission_tx.clone());

                let (_, writer) = stream.into_split();
                tokio::spawn(Self::handle_writing(hmac_system_key, writer, connection_rx));
                tokio::spawn(Self::handle_retransmission(connection_tx, retransmission_rx));
            }
        }

        RegisterClientState {
            processes_count,
            processes_txs,
            retransmission_txs,
        }
    }

    async fn handle_writing(
        hmac_system_key: [u8; 64],
        mut writer: OwnedWriteHalf, 
        rx: Receiver<SystemRegisterCommand>,
    ) {
        // Since only this task owns permission to write to the stream, if something was received 
        // on the channel that we want to send, call serialize_register_command and write to the stream.
        while let Ok(msg) = rx.recv().await {
            let cmd = RegisterCommand::System(msg);
            let _ = serialize_register_command(&cmd, &mut writer, &hmac_system_key).await;
        }
    }

    async fn handle_retransmission(
        connection_tx: Sender<SystemRegisterCommand>,
        retransmission_rx: Receiver<SystemRegisterCommand>,
    ) {
        let mut interval = interval_at(Instant::now() + DELAY, DELAY);
        let mut cmd: Option<SystemRegisterCommand> = None;

        loop {
            tokio::select! {
                biased;
                _ = interval.tick() => {
                    if let Some(command) = &cmd {
                        connection_tx.send(command.clone()).await.expect("Failed to pass the retransmission message to sender task");
                    }
                }
                msg = retransmission_rx.recv() => {
                    cmd = msg.ok();
                    interval = interval_at(Instant::now() + DELAY, DELAY);
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl RegisterClient for RegisterClientState {
    async fn send(&self, msg: Send) {
        let target = msg.target;
        let cmd = msg.cmd;

        let channel = self.processes_txs.get(&target).expect("No such process");
        channel.send((*cmd).clone()).await.expect("Failed to pass the message to sender task");

        let retransmission_channel = self.retransmission_txs.get(&target).expect("No such process");
        retransmission_channel.send((*cmd).clone()).await.expect("Failed to pass the message to retransmission task");
    }

    async fn broadcast(&self, msg: Broadcast) {
        let cmd = msg.cmd;

        for target in 1..=self.processes_count {
            let channel = self.processes_txs.get(&target).expect("No such process");
            channel.send((*cmd).clone()).await.expect("Failed to pass the message to sender task");

            let retransmission_channel = self.retransmission_txs.get(&target).expect("No such process");
            retransmission_channel.send((*cmd).clone()).await.expect("Failed to pass the message to retransmission task");
        }
    }
}
