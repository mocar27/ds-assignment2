// Register client odpowiada za komunikacje, 
// ale mozna myslec o nim jako o zbiorze stubborn linkow 
// do kazdego innego procesu i mi taka abstrakcja siada w glowie

// RegisterClient manages TCP communication between processes of the distributed register. 
// An instance is passed to instances of AtomicRegister to allow them communicating with each other.

// 1. Messages sent by a process to itself should skip TCP, serialization, deserialization, 
// HMAC preparation and validation phases to improve the performance

// 2. There is a limit on the number of open file descriptors: 1024. 
// We suggest utilizing it for maximum concurrency. 
// There will not be more than 16 client connections.


use std::sync::Arc;
use async_channel::{bounded, unbounded, Receiver, Sender};
use tokio::net::TcpStream;

use crate::{SectorIdx, RegisterClient, RegisterCommand, SystemRegisterCommand,
    Send, Broadcast, serialize_register_command, deserialize_register_command};


pub struct RegisterClientState {

}

impl RegisterClientState {
    pub fn new() -> Self {
        RegisterClientState {}
    }
}

#[async_trait::async_trait]
impl RegisterClient for RegisterClientState {
    async fn send(&self, msg: Send) {
        unimplemented!()
    }

    async fn broadcast(&self, msg: Broadcast) {
        unimplemented!()
    }
}
