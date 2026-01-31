// src/video/wrappers.rs

use rsmpeg::ffi;
use std::ptr;

// --- AVPacket Wrapper ---
pub struct SafePacket {
    pub ptr: *mut ffi::AVPacket,
}

impl SafePacket {
    pub fn new() -> Self {
        unsafe {
            let ptr = ffi::av_packet_alloc();
            Self { ptr }
        }
    }
}

impl Drop for SafePacket {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                ffi::av_packet_free(&mut self.ptr);
            }
        }
    }
}

// --- AVFrame Wrapper ---
pub struct SafeFrame {
    pub ptr: *mut ffi::AVFrame,
}

impl SafeFrame {
    pub fn new() -> Self {
        unsafe {
            let ptr = ffi::av_frame_alloc();
            Self { ptr }
        }
    }
}

impl Drop for SafeFrame {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                ffi::av_frame_free(&mut self.ptr);
            }
        }
    }
}

// --- AVFormatContext (Input) Wrapper ---
pub struct SafeFormatContextInput {
    pub ptr: *mut ffi::AVFormatContext,
}

impl SafeFormatContextInput {
    pub fn new() -> Self {
        Self { ptr: ptr::null_mut() }
    }
}

impl Drop for SafeFormatContextInput {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                ffi::avformat_close_input(&mut self.ptr);
            }
        }
    }
}

// --- AVFormatContext (Output) Wrapper ---
pub struct SafeFormatContextOutput {
    pub ptr: *mut ffi::AVFormatContext,
}

impl SafeFormatContextOutput {
    pub fn new() -> Self {
        Self { ptr: ptr::null_mut() }
    }
}

impl Drop for SafeFormatContextOutput {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                // For output, we usually do avformat_free_context if we allocated it,
                // BUT if we opened file (AVIO), we also need to close it.
                // The main flow usually closes pb and frees context.
                
                // Note: If avformat_write_header wasn't called or failed, we just free.
                // If we aren't sure, we should follow manual cleanup or be very careful.
                // Standard: avformat_free_context handles the structure.
                
                // If we opened the IO context (pb), we must close it separately usually
                // via avio_closep. 
                
                if !(*self.ptr).pb.is_null() {
                     // Check if we should close it (standard flags check)
                     if ((*(*self.ptr).oformat).flags as i32 & ffi::AVFMT_NOFILE as i32) == 0 {
                          ffi::avio_closep(&mut (*self.ptr).pb);
                     }
                }
                
                ffi::avformat_free_context(self.ptr);
            }
        }
    }
}


// --- AVCodecContext Wrapper ---
pub struct SafeCodecContext {
    pub ptr: *mut ffi::AVCodecContext,
}

impl SafeCodecContext {
    pub fn new(codec: *const ffi::AVCodec) -> Self {
        unsafe {
            let ptr = ffi::avcodec_alloc_context3(codec);
            Self { ptr }
        }
    }
}

impl Drop for SafeCodecContext {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                ffi::avcodec_free_context(&mut self.ptr);
            }
        }
    }
}

// --- SwsContext Wrapper ---
pub struct SafeSwsContext {
    pub ptr: *mut ffi::SwsContext,
}

impl SafeSwsContext {
    pub fn new(
        src_w: i32, src_h: i32, src_fmt: i32,
        dst_w: i32, dst_h: i32, dst_fmt: i32,
        flags: i32
    ) -> Self {
        unsafe {
            let ptr = ffi::sws_getContext(
                src_w, src_h, src_fmt,
                dst_w, dst_h, dst_fmt,
                flags, ptr::null_mut(), ptr::null_mut(), ptr::null()
            );
            Self { ptr }
        }
    }
}

impl Drop for SafeSwsContext {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                ffi::sws_freeContext(self.ptr);
            }
        }
    }
}
