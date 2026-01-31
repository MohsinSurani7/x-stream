// src/video/types.rs

#[derive(Debug, Clone)]
pub struct RawFrame {
    pub data: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub pts: i64,
}

#[derive(Debug, Clone)]
pub struct UpscaledFrame {
    pub data: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub pts: i64,
}

#[derive(Debug, Clone)]
pub struct PacketData {
    pub data: Vec<u8>,
    pub pts: i64,
    pub dts: i64,
    pub stream_index: i32,
    pub flags: i32,
    pub duration: i64,
    pub pos: i64,
}

pub enum DecoderMsg {
    Video(RawFrame),
    Audio(PacketData),
    EOF,
}

pub enum EncoderMsg {
    Video(UpscaledFrame),
    Audio(PacketData),
    EOF,
}
