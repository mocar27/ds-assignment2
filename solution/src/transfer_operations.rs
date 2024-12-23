// Serialize & Deserialize data operations
// They convert bytes to a RegisterCommand object and in the other direction, respectively. 
// They shall implement the message formats as described above (see the description of TCP communication).


use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::convert::TryInto;

use crate::{MAGIC_NUMBER, SectorIdx, SectorVec, 
    RegisterCommand, ClientRegisterCommand, ClientCommandHeader, ClientRegisterCommandContent, 
    SystemRegisterCommand, SystemCommandHeader, SystemRegisterCommandContent};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
pub enum SerializationError {
    Io(std::io::Error),
    InvalidHMAC,
    InvalidMagicNumber,
    InvalidMessageType,
}

// Client to process message types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum C2PMessageType {
    Read = 0x01,
    Write = 0x02,
    
    ReadResponse = 0x41,
    WriteResponse = 0x42,
}

// Process to process message types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum P2PMessageType {
    ReadProc = 0x03,
    Value =  0x04,
    WriteProc = 0x05,
    Ack = 0x06,

    ReadProcResponse = 0x43,
    ValueResponse = 0x44,
    WriteProcResponse = 0x45,
    AckResponse = 0x46,
}

