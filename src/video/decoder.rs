// src/video/decoder.rs

use rsmpeg::ffi;
use std::ffi::CString;
use std::ptr;
use anyhow::{Result, anyhow};
use crossbeam_channel::Sender;
use crate::video::types::{DecoderMsg, EncoderMsg, RawFrame, PacketData};

pub fn run_decoder(
    input_path: &str,
    tx_video_raw: Sender<DecoderMsg>,
    tx_encoder_audio: Sender<EncoderMsg>,
) -> Result<()> {
    unsafe {
        let in_c = CString::new(input_path).unwrap();
        
        let mut safefmt = crate::video::wrappers::SafeFormatContextInput::new();
        
        if ffi::avformat_open_input(&mut safefmt.ptr, in_c.as_ptr(), ptr::null_mut(), ptr::null_mut()) < 0 {
             return Err(anyhow!("FFmpeg: Open input failed: {}", input_path));
        }
        ffi::avformat_find_stream_info(safefmt.ptr, ptr::null_mut());
        
        let mut video_stream_idx = -1;
        let mut audio_stream_idx = -1;
        
        for i in 0..(*safefmt.ptr).nb_streams {
             let stream = *(*safefmt.ptr).streams.add(i as usize);
             let codec_type = (*(*stream).codecpar).codec_type;
             if codec_type == ffi::AVMEDIA_TYPE_VIDEO {
                 video_stream_idx = i as i32;
             } else if codec_type == ffi::AVMEDIA_TYPE_AUDIO {
                 audio_stream_idx = i as i32;
             }
        }
        
        if video_stream_idx == -1 {
            return Err(anyhow!("No video stream found"));
        }
        
        // Setup Video Decoder
        let in_stream = *(*safefmt.ptr).streams.add(video_stream_idx as usize);
        let decoder = ffi::avcodec_find_decoder((*(*in_stream).codecpar).codec_id);
        
        let mut safe_decode_ctx = crate::video::wrappers::SafeCodecContext::new(decoder);
        ffi::avcodec_parameters_to_context(safe_decode_ctx.ptr, (*in_stream).codecpar);
        ffi::avcodec_open2(safe_decode_ctx.ptr, decoder, ptr::null_mut());
        
        // Setup SWS Context (YUV/etc -> RGB)
        let safe_sws = crate::video::wrappers::SafeSwsContext::new(
            (*safe_decode_ctx.ptr).width, (*safe_decode_ctx.ptr).height, (*safe_decode_ctx.ptr).pix_fmt,
            (*safe_decode_ctx.ptr).width, (*safe_decode_ctx.ptr).height, ffi::AV_PIX_FMT_RGB24,
            ffi::SWS_BILINEAR as i32
        );
        
        let mut safe_pkt = crate::video::wrappers::SafePacket::new();
        let mut safe_frame = crate::video::wrappers::SafeFrame::new();
        
        let mut pts_counter = 0;
        
        while ffi::av_read_frame(safefmt.ptr, safe_pkt.ptr) >= 0 {
            if (*safe_pkt.ptr).stream_index == video_stream_idx {
                if ffi::avcodec_send_packet(safe_decode_ctx.ptr, safe_pkt.ptr) >= 0 {
                    while ffi::avcodec_receive_frame(safe_decode_ctx.ptr, safe_frame.ptr) == 0 {
                        // Convert to RGB
                        let mut rgb_data = vec![0u8; ((*safe_frame.ptr).width * (*safe_frame.ptr).height * 3) as usize];
                        let mut rgb_ptr = [rgb_data.as_mut_ptr(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()];
                        let mut rgb_linesize = [((*safe_frame.ptr).width * 3) as i32, 0, 0, 0];
                        ffi::sws_scale(
                            safe_sws.ptr, (*safe_frame.ptr).data.as_ptr() as *const *const u8, (*safe_frame.ptr).linesize.as_ptr(),
                            0, (*safe_frame.ptr).height, rgb_ptr.as_mut_ptr(), rgb_linesize.as_mut_ptr()
                        );
                        
                        let raw = RawFrame {
                            data: rgb_data,
                            width: (*safe_frame.ptr).width,
                            height: (*safe_frame.ptr).height,
                            pts: pts_counter,
                        };
                        pts_counter += 1;
                        
                        if let Err(_) = tx_video_raw.send(DecoderMsg::Video(raw)) {
                            break; 
                        }
                    }
                }
            } else if (*safe_pkt.ptr).stream_index == audio_stream_idx {
                // Copy Audio Packet
                let size = (*safe_pkt.ptr).size as usize;
                let mut data_vec = vec![0u8; size];
                ptr::copy_nonoverlapping((*safe_pkt.ptr).data, data_vec.as_mut_ptr(), size);
                
                let p_data = PacketData {
                    data: data_vec,
                    pts: (*safe_pkt.ptr).pts,
                    dts: (*safe_pkt.ptr).dts,
                    stream_index: (*safe_pkt.ptr).stream_index,
                    flags: (*safe_pkt.ptr).flags,
                    duration: (*safe_pkt.ptr).duration,
                    pos: (*safe_pkt.ptr).pos,
                };
                if let Err(_) = tx_encoder_audio.send(EncoderMsg::Audio(p_data)) {
                     break; 
                }
            }
            ffi::av_packet_unref(safe_pkt.ptr);
        }
        
        let _ = tx_video_raw.send(DecoderMsg::EOF);
        
        // No manual cleanup needed! Drop traits handle it.
    }
    Ok(())
}
