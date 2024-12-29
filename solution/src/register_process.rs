// Register process jest po to, ze jak masz te funkcje w lib.rs run_register_process, 
// to pewnie chcialbys miec jakis stan/zmienne/cos i ta funkcja moze urosnac porzadnie, 
// wiec ja to sobie podzielilem na structa, wiec robie cos typu
// let rp = RegisterProcess::new(…);
// rp.run().await()

// Handling messages from processes and from myself.

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;

use async_channel::Receiver;
use hmac::{Mac, Hmac};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use std::io::Error;
use tokio::net::TcpStream;

use crate::{AtomicRegister, atomic_register::AtomicRegisterState, build_atomic_register, 
    Configuration, PublicConfiguration, SectorIdx, SectorVec, StatusCode,
    RegisterCommand, OperationReturn, OperationSuccess, ReadReturn, 
    ClientRegisterCommand, ClientCommandHeader, ClientRegisterCommandContent, 
    SystemRegisterCommand, SystemCommandHeader, SystemRegisterCommandContent, 
    RegisterClient, SectorsManager,
    serialize_register_command, deserialize_register_command};

pub struct RegisterProcessState {

}
