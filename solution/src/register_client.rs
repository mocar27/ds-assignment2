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
use std::sync::Arc;
use async_channel::{Receiver, Sender, unbounded};
use tokio::net::TcpStream;

use crate::{SectorIdx, RegisterClient, SystemRegisterCommand,
    Send, Broadcast, serialize_register_command};


pub struct RegisterClientState {
    // Metadata needed to send TCP commands to other processes.
    hmac_system_key: [u8; 64],
    processes_count: u8,

    // Channel for giving the information to task handlers that we want to send a message to some process.
    // Later the task will consume the message on the channel and send it to the target process.
    // This way we don't pass the sending socket to every task handler in the system.
    processes_txs: HashMap<u8, Sender<SystemRegisterCommand>>,

    // Channel inside which we put messages that potentially would need to be retransmitted.
    retransmission_messages_tx: Sender<Box<SystemRegisterCommand>>,
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
        processes_txs.insert(self_ident, self_tx);

        let (retransmission_messages_tx, retransmission_messages_rx) = unbounded();

        for i in 0..processes_count {
            // Adress of each process is under tcp_locations[i - 1]
            // Create a connection to every other process in the system.
            if i != self_ident - 1 {
                let addr = &tcp_locations[i as usize];
                let stream = TcpStream::connect(addr).await.expect("Failed to connect to process");
                let (tx, rx) = unbounded();
                
                processes_txs.insert(i + 1, tx);

                tokio::spawn(Self::handle_connection(stream, rx));
            }
        }

        // Spawn task that will handle retransmissions.
        tokio::spawn(async move {
            loop {
                let msg = retransmission_messages_rx.recv().await.unwrap();
                // Handle retransmission
            }
        });

        RegisterClientState {
            hmac_system_key,
            processes_count,
            processes_txs,
            retransmission_messages_tx,
        }
    }

    // Function that will handle connection with other processes.
    // It will be responsible for sending messages to other processes.
    // It will also be responsible for receiving messages from other processes.
    async fn handle_connection(stream: TcpStream, rx: Receiver<SystemRegisterCommand>) {
        // Spawn task that will handle sending messages to other processes.
        stream.set_nodelay(true).expect("Failed to set nodelay");

        // tokio::spawn(async move {
        //     loop {
        //         let msg = rx.recv().await.unwrap();
        //         let serialized_msg = serialize_register_command(&msg);
        //         stream.write_all(&serialized_msg).await.expect("Failed to write to stream");
        //     }
        // });

        // Spawn task that will handle receiving messages from other processes.
        tokio::spawn(async move {
            loop {
                // Read message from stream
                // Deserialize message
                // Pass message to self_tx
            }
        });
    }

    // For receiving messages will be responsible other spawned task.
    // function to handle receiving messages from other processes and another one for retransmissions receiving
    // For example 500 ms is reasonable (for retransmissions).
    // potentially move retransmission_txs to another function and tokio task that will work for given u8

    // We cannot lose any messages, even when a target process crashes and the TCP connection gets broken.
    // If we don't get acknowledgement that process has received the message (after 500 ms) we will begin retransmissions every 500 ms.
    // If we get on the receiver that process has recovered, we can stop retransmissions.
    // Channel for inserting messages that potentially would need to be retransmitted is defined in self.
}

#[async_trait::async_trait]
impl RegisterClient for RegisterClientState {
    async fn send(&self, msg: Send) {
        let target = msg.target;
        let cmd = msg.cmd;

        let channel = self.processes_txs.get(&target).expect("No such process");
        channel.send((*cmd).clone()).await.expect("Failed to pass the message to sender task");
    }

    async fn broadcast(&self, msg: Broadcast) {
        let cmd = msg.cmd;

        for target in 1..=self.processes_count {
            let channel = self.processes_txs.get(&target).expect("No such process");
            channel.send((*cmd).clone()).await.expect("Failed to pass the message to sender task");
        }
    }
}
