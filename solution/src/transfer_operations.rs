// Serialize & Deserialize data operations

use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::convert::TryFrom;
use std::io::Error;

use crate::{utils::SerializationError, MAGIC_NUMBER, SectorIdx, SectorVec, StatusCode, 
    RegisterCommand, ClientRegisterCommand, ClientCommandHeader, ClientRegisterCommandContent, 
    SystemRegisterCommand, SystemCommandHeader, SystemRegisterCommandContent};

type HmacSha256 = Hmac<Sha256>;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    // Client to process message types (Client)
    Read = 0x01,
    Write = 0x02,

    // Process to process message types (System)
    ReadProc = 0x03,
    Value =  0x04,
    WriteProc = 0x05,
    Ack = 0x06,
    // We don't create reponse types, as we will do it on the fly adding 0x40 to original message
    // For the sake of simplicity, we will not divide it into two different enums.
}

impl TryFrom<u8> for MessageType {
    type Error = SerializationError;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0x01 => Ok(MessageType::Read),
            0x02 => Ok(MessageType::Write),
            0x03 => Ok(MessageType::ReadProc),
            0x04 => Ok(MessageType::Value),
            0x05 => Ok(MessageType::WriteProc),
            0x06 => Ok(MessageType::Ack),
            _ => Err(SerializationError::InvalidMessageType),
        }
    }
}

pub fn construct_client_message() {
    unimplemented!()
}

pub fn construct_system_message() {
    unimplemented!()
}

pub fn construct_advanced_system_message() {
    unimplemented!()
}

// Bytes (received on input) -> RegisterCommand (Client or System)
pub async fn deserialize_rc(
    data: &mut (dyn AsyncRead + Send + Unpin),
    hmac_system_key: &[u8; 64],
    hmac_client_key: &[u8; 32],
) -> Result<(RegisterCommand, bool), Error> {
    
    // MAGIC_NUMBER [0..4], Padding [4..6], ProcessRank (only if msg of type P2P) [6], MsgType [7]  
    let mut valid_magic_number=  [0u8; 4];
    data.read_exact(&mut valid_magic_number).await?;

    // The solution shall slide over bytes in the stream until it detects a valid magic number. 
    // This marks the beginning of a message.
    while valid_magic_number != MAGIC_NUMBER {
        valid_magic_number.copy_within(1..4, 0);
        data.read_exact(&mut valid_magic_number[3..4]).await?;
    }

    let mut buff = [0u8; 4];
    data.read_exact(&mut buff).await?;

    // Magic number correct, skipping padding => msg_type is 4th buff element.
    let msg_type = MessageType::try_from(buff[3]).unwrap();

    // If a message type is invalid, the solution shall discard the magic number and the following 4 bytes (8 bytes in total).
    match msg_type {
        MessageType::Read| MessageType::Write => {
            // Client to process message
            // Read the rest of the message and validate HMAC
            // If the HMAC is invalid, the solution shall return an error.
            unimplemented!()
        },
        MessageType::ReadProc | MessageType::Ack => {
            // Process to process message
            // Read the rest of the message and validate HMAC
            // If the HMAC is invalid, the solution shall return an error.
            unimplemented!()
        },
        MessageType::Value | MessageType::WriteProc => {
            // Process to process message (advanced)
            // Read the rest of the message and validate HMAC
            // If the HMAC is invalid, the solution shall return an error.
            unimplemented!()
        },
    }



    // Deserialization shall return a pair (message, hmac_valid) 
    // when a valid magic number and a valid message type is encountered. 

    // return 
}

// RegisterCommand (received on input) -> bytes (written to writer recived on input)
pub async fn serialize_rc(
    cmd: &RegisterCommand,
    writer: &mut (dyn AsyncWrite + Send + Unpin),
    hmac_key: &[u8],
) -> Result<(), Error> {

    // Serialization shall complete successfully when there are no errors 
    // when writing to the provided reference that implements AsyncWrite. 

    // Read the RegisterCommand and construct bytes data from it (message content descirbed in task description)

    // If errors occur, the serializing function shall return them.

    unimplemented!() 
}

