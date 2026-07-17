use std::io::Read;

pub const MAGIC: u32 = 0x434C4950;
pub const TYPE_TEXT: u8 = 0x01;
pub const TYPE_IMAGE: u8 = 0x02;
pub const TYPE_FILE: u8 = 0x03;
pub const TYPE_HEARTBEAT: u8 = 0x04;
pub const TYPE_ACK: u8 = 0x05;
pub const CHUNK_SIZE: usize = 65536;
pub const HEADER_SIZE: usize = 12;
pub const COMPRESSED: u8 = 0x01;
pub const NOT_COMPRESSED: u8 = 0x00;

#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub magic: u32,
    pub msg_type: u8,
    pub compressed: u8,
    pub data_len: u32,
    pub sequence: u16,
    pub reserved: u8,
}

impl MessageHeader {
    pub fn new(msg_type: u8, data_len: u32, sequence: u16) -> Self {
        Self {
            magic: MAGIC,
            msg_type,
            compressed: NOT_COMPRESSED,
            data_len,
            sequence,
            reserved: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_be_bytes());
        buf[4] = self.msg_type;
        buf[5] = self.compressed;
        buf[6..10].copy_from_slice(&self.data_len.to_be_bytes());
        buf[10..12].copy_from_slice(&self.sequence.to_be_bytes());
        buf
    }

    pub fn from_reader(reader: &mut impl Read) -> std::io::Result<Self> {
        let mut buf = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buf)?;

        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid magic number",
            ));
        }

        Ok(Self {
            magic,
            msg_type: buf[4],
            compressed: buf[5],
            data_len: u32::from_be_bytes([buf[6], buf[7], buf[8], buf[9]]),
            sequence: u16::from_be_bytes([buf[10], buf[11]]),
            reserved: buf[11],
        })
    }
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub filename: String,
    pub file_size: u64,
}

impl FileMetadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = self.filename.as_bytes().to_vec();
        data.push(0);
        data.extend_from_slice(&self.file_size.to_be_bytes());
        data
    }

    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        let null_pos = slice.iter().position(|&b| b == 0)?;
        let filename = String::from_utf8(slice[..null_pos].to_vec()).ok()?;
        let size_bytes = slice.get(null_pos + 1..null_pos + 9)?;
        let file_size = u64::from_be_bytes(size_bytes.try_into().ok()?);
        Some(Self { filename, file_size })
    }
}

pub fn encode_file(filename: &str, contents: &[u8]) -> Result<Vec<u8>, String> {
    let name = std::path::Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "Invalid file name".to_string())?;
    let size = u64::try_from(contents.len()).map_err(|_| "File is too large")?;
    let metadata = FileMetadata { filename: name.to_string(), file_size: size };
    let mut payload = metadata.to_bytes();
    payload.extend_from_slice(contents);
    Ok(payload)
}

pub fn decode_file(payload: &[u8]) -> Result<(String, Vec<u8>), String> {
    let separator = payload.iter().position(|byte| *byte == 0).ok_or_else(|| "File metadata is incomplete".to_string())?;
    let metadata_end = separator.checked_add(9).ok_or_else(|| "File metadata is invalid".to_string())?;
    let metadata = FileMetadata::from_slice(payload).ok_or_else(|| "File metadata is invalid".to_string())?;
    let contents = payload.get(metadata_end..).ok_or_else(|| "File payload is incomplete".to_string())?;
    if metadata.file_size != contents.len() as u64 {
        return Err("File payload size does not match metadata".to_string());
    }
    Ok((metadata.filename, contents.to_vec()))
}

pub fn encode_image(width: usize, height: usize, bytes: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || bytes.len() != width.saturating_mul(height).saturating_mul(4) {
        return Err("Invalid RGBA image payload".to_string());
    }
    let width = u32::try_from(width).map_err(|_| "Image width is too large")?;
    let height = u32::try_from(height).map_err(|_| "Image height is too large")?;
    let mut payload = Vec::with_capacity(8 + bytes.len());
    payload.extend_from_slice(&width.to_be_bytes());
    payload.extend_from_slice(&height.to_be_bytes());
    payload.extend_from_slice(bytes);
    Ok(payload)
}

pub fn decode_image(payload: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    if payload.len() < 8 {
        return Err("Image payload is incomplete".to_string());
    }
    let width = u32::from_be_bytes(payload[0..4].try_into().unwrap()) as usize;
    let height = u32::from_be_bytes(payload[4..8].try_into().unwrap()) as usize;
    let bytes = payload[8..].to_vec();
    if width == 0 || height == 0 || bytes.len() != width.saturating_mul(height).saturating_mul(4) {
        return Err("Image payload is invalid".to_string());
    }
    Ok((width, height, bytes))
}
