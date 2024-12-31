// Serialize & Deserialize data operations for RegisterCommand.

use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::convert::TryFrom;
use std::io::Error;

use crate::{utils::SerializationError, MAGIC_NUMBER, SectorIdx, SectorVec, 
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

// Mac verification as in dslab05, we don't calculate the HMAC tag, as it is part of the deserialized message.
// We read the data and update mac on the spot to later compare it with the received tag (as described in lab).
// When invalid HMAC tag is detected, we will still parse whole message, 
// but we will return the false flag in the tuple.
async fn construct_client_command(
    data: &mut (dyn AsyncRead + Send + Unpin),
    mac: &mut HmacSha256,
    msg_type: MessageType,
) -> Result<(RegisterCommand, bool), Error> {
    let mut buff = [0u8; 8];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let rid: u64 = u64::from_be_bytes(buff);

    let mut buff = [0u8; 8];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let sidx: SectorIdx = u64::from_be_bytes(buff);

    let header = ClientCommandHeader {
        request_identifier: rid,
        sector_idx: sidx,
    };

    let content = match msg_type {
        MessageType::Read => ClientRegisterCommandContent::Read,
        MessageType::Write => {
            let mut buff = [0u8; 4096];
            data.read_exact(&mut buff).await?;
            mac.update(&buff);
            ClientRegisterCommandContent::Write { data: SectorVec(buff.to_vec()) }
        },
        _ => Err(SerializationError::InvaildData)?,
    }; 

    let mut hmac_tag = [0u8; 32];
    data.read_exact(&mut hmac_tag).await?;

    let is_valid = mac.clone().verify_slice(&hmac_tag).is_ok();
    Ok((RegisterCommand::Client(ClientRegisterCommand { header, content }), is_valid))
}

async fn construct_system_command(
    data: &mut (dyn AsyncRead + Send + Unpin),
    mac: &mut HmacSha256,
    process_rank: u8,
    msg_type: MessageType,
) -> Result<(RegisterCommand, bool), Error> {
    let mut buff = [0u8; 16];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let uid = uuid::Uuid::from_slice(&buff).unwrap();

    let mut buff = [0u8; 8];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let sidx: SectorIdx = u64::from_be_bytes(buff);
    
    let mut hmac_tag = [0u8; 32];
    data.read_exact(&mut hmac_tag).await?;
    
    let header = SystemCommandHeader {
        process_identifier: process_rank,
        msg_ident: uid,
        sector_idx: sidx,
    };

    let content = match msg_type {
        MessageType::ReadProc => SystemRegisterCommandContent::ReadProc,
        MessageType::Ack => SystemRegisterCommandContent::Ack,
        _ => Err(SerializationError::InvaildData)?,
    };

    let is_valid = mac.clone().verify_slice(&hmac_tag).is_ok();
    Ok((RegisterCommand::System(SystemRegisterCommand { header, content }), is_valid))
}

async fn construct_advanced_system_command(
    data: &mut (dyn AsyncRead + Send + Unpin),
    mac: &mut HmacSha256,
    process_rank: u8,
    msg_type: MessageType,
) -> Result<(RegisterCommand, bool), Error> {
    let mut buff = [0u8; 16];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let uid = uuid::Uuid::from_slice(&buff).unwrap();

    let mut buff = [0u8; 8];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let sidx: SectorIdx = u64::from_be_bytes(buff);

    let mut buff = [0u8; 8];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let ts = u64::from_be_bytes(buff);

    let mut buff = [0u8; 8];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let value_wr = buff[7];

    let mut buff = [0u8; 4096];
    data.read_exact(&mut buff).await?;
    mac.update(&buff);
    let svec = SectorVec(buff.to_vec());

    let mut hmac_tag = [0u8; 32];
    data.read_exact(&mut hmac_tag).await?;

    let header = SystemCommandHeader {
        process_identifier: process_rank,
        msg_ident: uid,
        sector_idx: sidx,
    };

    let content = match msg_type {
        MessageType::Value => SystemRegisterCommandContent::Value { 
            timestamp: ts, 
            write_rank: value_wr, 
            sector_data: svec,
        },
        MessageType::WriteProc => SystemRegisterCommandContent::WriteProc { 
            timestamp: ts, 
            write_rank: value_wr, 
            data_to_write: svec,
        },
        _ => Err(SerializationError::InvaildData)?,
    };

    let is_valid = mac.clone().verify_slice(&hmac_tag).is_ok();
    Ok((RegisterCommand::System(SystemRegisterCommand { header, content }), is_valid))
}

// Bytes (received on input) -> RegisterCommand (Client or System)
pub async fn deserialize_rc(
    data: &mut (dyn AsyncRead + Send + Unpin),
    hmac_system_key: &[u8; 64],
    hmac_client_key: &[u8; 32],
) -> Result<(RegisterCommand, bool), Error> {
    
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
    // Process rank is 3rd element of the buffer (it existsts only for process - process communication).
    // If a message type is invalid, the solution will discard the magic number and the following 4 bytes (8 bytes in total).
    // It will throw the error here, after already reading the 8 bytes.
    let msg_type = MessageType::try_from(buff[3]).unwrap();

    // The HMAC tag is a hmac(sh256) tag of the entire message (from the magic number to the end of the content),
    // so we construct the initial mac here with already read data.
    match msg_type {
        MessageType::Read| MessageType::Write => {
            // Client to process message
            let mut mac = HmacSha256::new_from_slice(hmac_client_key).unwrap();
            mac.update(&valid_magic_number);
            mac.update(&buff);
            construct_client_command(data, &mut mac, msg_type).await
        },
        MessageType::ReadProc | MessageType::Ack => {
            // Process to process message
            let mut mac = HmacSha256::new_from_slice(hmac_system_key).unwrap();
            mac.update(&valid_magic_number);
            mac.update(&buff);
            construct_system_command(data, &mut mac, buff[2], msg_type).await
        },
        MessageType::Value | MessageType::WriteProc => {
            // Process to process message (advanced - response is different)
            let mut mac = HmacSha256::new_from_slice(hmac_system_key).unwrap();
            mac.update(&valid_magic_number);
            mac.update(&buff);
            construct_advanced_system_command(data, &mut mac, buff[2], msg_type).await
        },
    }
}

// Serialization shall complete successfully when there are no errors 
// when writing to the provided reference that implements AsyncWrite. 
// (it means that if there is error, return it immediately)
// Read the RegisterCommand and construct bytes data from it (message content descirbed in task description)
// RegisterCommand (received on input) -> Bytes (written to writer recived on input)
async fn serialize_client_rc(
    header: &ClientCommandHeader,
    content: &ClientRegisterCommandContent
) -> Result<Vec<u8>, Error> {
    let mut buff = Vec::<u8>::new();
    buff.write_all(&MAGIC_NUMBER).await?;

    let msg_type = match content {
        ClientRegisterCommandContent::Read => MessageType::Read,
        ClientRegisterCommandContent::Write { data: _ } => MessageType::Write
    };

    // As MessageType is translatable to u8, we can write it directly to the buffer.
    // Writing padding and the message type to the buffer.
    buff.write_all(&[0, 0, 0, msg_type as u8]).await?;

    // Respectively, writing all the data contained in the header and the content of RegisterCommand.
    let rid = header.request_identifier.to_be_bytes();
    buff.write_all(&rid).await?;

    let sidx = header.sector_idx.to_be_bytes();
    buff.write_all(&sidx).await?;

    match content {
        ClientRegisterCommandContent::Read => (),
        ClientRegisterCommandContent::Write { data } => {
            buff.write_all(&data.0).await?;
        }
    }

    Ok(buff)
}

async fn serialize_system_rc(
    header: &SystemCommandHeader,
    content: &SystemRegisterCommandContent
) -> Result<Vec<u8>, Error> {
    let mut buff = Vec::<u8>::new();
    buff.write_all(&MAGIC_NUMBER).await?;

    let msg_type = match content {
        SystemRegisterCommandContent::ReadProc => MessageType::ReadProc,
        SystemRegisterCommandContent::Ack => MessageType::Ack,
        SystemRegisterCommandContent::Value { timestamp: _, write_rank: _, sector_data: _ } => MessageType::Value,
        SystemRegisterCommandContent::WriteProc { timestamp: _, write_rank: _, data_to_write: _ } => MessageType::WriteProc,
    };

    buff.write_all(&[0, 0, header.process_identifier, msg_type as u8]).await?;

    let uid = header.msg_ident.as_bytes();
    buff.write_all(uid).await?;

    let sidx = header.sector_idx.to_be_bytes();
    buff.write_all(&sidx).await?;

    match content {
        SystemRegisterCommandContent::ReadProc | SystemRegisterCommandContent::Ack => (),
        SystemRegisterCommandContent::Value { timestamp, write_rank, sector_data } => {
            let ts = timestamp.to_be_bytes();
            buff.write_all(&ts).await?;

            let wr = write_rank.to_be_bytes();
            buff.write_all(&[0, 0, 0, 0, 0, 0, 0, wr[0]]).await?;
            
            buff.write_all(&sector_data.0).await?;
        },
        SystemRegisterCommandContent::WriteProc { timestamp, write_rank, data_to_write } => {
            let ts = timestamp.to_be_bytes();
            buff.write_all(&ts).await?;

            let wr = write_rank.to_be_bytes();
            buff.write_all(&[0, 0, 0, 0, 0, 0, 0, wr[0]]).await?;


            buff.write_all(&data_to_write.0).await?;
        },
    }

    Ok(buff)
}

// RegisterCommand (received on input) -> bytes (written to writer recived on input)
pub async fn serialize_rc(
    cmd: &RegisterCommand,
    writer: &mut (dyn AsyncWrite + Send + Unpin),
    hmac_key: &[u8],
) -> Result<(), Error> {

    let mut data = match cmd {
        RegisterCommand::Client(ClientRegisterCommand { header, content}) => {
            serialize_client_rc(header, content).await?
        }
        RegisterCommand::System(SystemRegisterCommand { header, content}) => {
            serialize_system_rc(header, content).await?
        }
    };

    let mut mac = HmacSha256::new_from_slice(hmac_key).unwrap();
    mac.update(&data);
    let tag: [u8; 32] = mac.finalize().into_bytes().as_slice().try_into().unwrap();
    data.write_all(&tag).await?;

    writer.write_all(&data).await?;
    Ok(())
}
 