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
use std::sync::{Arc, Mutex};
use async_channel::{bounded, unbounded, Receiver, Sender};
use tokio::net::TcpStream;

use crate::{SectorIdx, RegisterClient, RegisterCommand, SystemRegisterCommand,
    Send, Broadcast, serialize_register_command};


pub struct RegisterClientState {
    // Metadata needed to communicate with other processes.
    self_ident: u8,
    hmac_system_key: [u8; 64],
    processes_count: u8,

    // As messages to itself should skip TCP, we will insert them to internal channel.
    self_tx: Sender<Box<SystemRegisterCommand>>,

    // Channel for sending messages to other processes.
    processes_txs: Vec<Sender<Box<SystemRegisterCommand>>>,

    // Channel inside which we put messages that potentially would need to be retransmitted.
    retransmission_messages_tx: Sender<Box<SystemRegisterCommand>>,
}

impl RegisterClientState {
    pub fn new() -> Self {
        RegisterClientState {

        }
        // Host and port, indexed by identifiers, of every process, including itself
        // (subtract 1 from self_rank to obtain index in this array).
        // You can assume that `tcp_locations.len() < 255`.
        // pub tcp_locations: Vec<(String, u16)>,
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
        // 1. Messages sent by a process to itself should skip TCP, serialization, deserialization, 
        // HMAC preparation and validation phases to improve the performance
        unimplemented!()
    }

    async fn broadcast(&self, msg: Broadcast) {
        unimplemented!()
    }
}
