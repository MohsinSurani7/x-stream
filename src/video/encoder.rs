// src/video/encoder.rs

use rsmpeg::ffi;
use std::ffi::CString;
use std::ptr;
use anyhow::{Result, anyhow};
use crossbeam_channel::Receiver;
use std::io::Write;
use crate::video::types::{EncoderMsg};

pub struct EncoderConfig {
    pub width: i32,
    pub height: i32,
    pub time_base: ffi::AVRational,
    pub bitrate: i64,
}

pub fn run_encoder(
    input_path: &str, // Needed to copy audio params
    output_path: &str,
    rx_encoder: Receiver<EncoderMsg>,
    config: EncoderConfig,
) -> Result<()> {
    unsafe {
        let out_str_c = CString::new(output_path).unwrap();
        let in_str = input_path.to_string();
        
        let mut safe_out_ctx = crate::video::wrappers::SafeFormatContextOutput::new();
        ffi::avformat_alloc_output_context2(&mut safe_out_ctx.ptr, ptr::null_mut(), ptr::null_mut(), out_str_c.as_ptr());
        
        if safe_out_ctx.ptr.is_null() {
            return Err(anyhow!("Could not allocate output context"));
        }

        // Setup Video Stream
        let encoder = ffi::avcodec_find_encoder(ffi::AV_CODEC_ID_H264);
        let out_video_stream = ffi::avformat_new_stream(safe_out_ctx.ptr, ptr::null());
        
        let mut safe_encode_ctx = crate::video::wrappers::SafeCodecContext::new(encoder);
        (*safe_encode_ctx.ptr).width = config.width;
        (*safe_encode_ctx.ptr).height = config.height;
        (*safe_encode_ctx.ptr).time_base = config.time_base; 
        (*safe_encode_ctx.ptr).pix_fmt = ffi::AV_PIX_FMT_YUV420P;
        (*safe_encode_ctx.ptr).bit_rate = config.bitrate;
        (*safe_encode_ctx.ptr).rc_min_rate = config.bitrate;
        (*safe_encode_ctx.ptr).rc_max_rate = config.bitrate;
        (*safe_encode_ctx.ptr).rc_buffer_size = (config.bitrate / 2) as i32;

        ffi::avcodec_parameters_from_context((*out_video_stream).codecpar, safe_encode_ctx.ptr);
        if ffi::avcodec_open2(safe_encode_ctx.ptr, encoder, ptr::null_mut()) < 0 {
             return Err(anyhow!("Failed to open video encoder"));
        }

        // Setup Audio Stream (Copy parms from input)
        // Note: We use a temporary Safe input context just to read params, it will auto-close!
        let mut out_audio_stream: *mut ffi::AVStream = ptr::null_mut();
        let mut input_audio_tb = ffi::AVRational{num:1,den:1};
        {
             let in_c_temp = CString::new(in_str).unwrap();
             let mut safe_temp_fmt = crate::video::wrappers::SafeFormatContextInput::new();
             ffi::avformat_open_input(&mut safe_temp_fmt.ptr, in_c_temp.as_ptr(), ptr::null_mut(), ptr::null_mut());
             ffi::avformat_find_stream_info(safe_temp_fmt.ptr, ptr::null_mut());
             
             let mut audio_stream_idx = -1;
             for i in 0..(*safe_temp_fmt.ptr).nb_streams {
                  if (*(*(*(*safe_temp_fmt.ptr).streams.add(i as usize))).codecpar).codec_type == ffi::AVMEDIA_TYPE_AUDIO {
                      audio_stream_idx = i as i32;
                      break;
                  }
             }
             
             if audio_stream_idx != -1 {
                  let s = *(*safe_temp_fmt.ptr).streams.add(audio_stream_idx as usize);
                  out_audio_stream = ffi::avformat_new_stream(safe_out_ctx.ptr, ptr::null());
                  ffi::avcodec_parameters_copy((*out_audio_stream).codecpar, (*s).codecpar);
                  (*(*out_audio_stream).codecpar).codec_tag = 0;
                  input_audio_tb = (*s).time_base;
             }
        } // safe_temp_fmt dropped here, closed safe.


        if ((*(*safe_out_ctx.ptr).oformat).flags as i32 & ffi::AVFMT_NOFILE as i32) == 0 {
            ffi::avio_open(&mut (*safe_out_ctx.ptr).pb, out_str_c.as_ptr(), ffi::AVIO_FLAG_WRITE as i32);
        }
        ffi::avformat_write_header(safe_out_ctx.ptr, ptr::null_mut());

        // Resources for Encoder
        let mut safe_out_frame = crate::video::wrappers::SafeFrame::new();
        (*safe_out_frame.ptr).width = config.width;
        (*safe_out_frame.ptr).height = config.height;
        (*safe_out_frame.ptr).format = ffi::AV_PIX_FMT_YUV420P as i32;
        ffi::av_frame_get_buffer(safe_out_frame.ptr, 32);
        
        let mut safe_pkt = crate::video::wrappers::SafePacket::new();

        let mut video_frames_done = 0;
        let mut total_processed: i64 = 0;

        loop {
            match rx_encoder.recv() {
                Ok(EncoderMsg::Video(up_frame)) => {
                    let w = config.width;
                    let h = config.height;
                    let y_size = (w * h) as usize;
                    let u_size = (w/2 * h/2) as usize;
                    let v_size = u_size;
                    
                    if up_frame.data.len() >= y_size + u_size + v_size {
                         // Copy Y
                         let src_y = &up_frame.data[0..y_size];
                         for i in 0..h {
                             let src_start = (i * w) as usize;
                             let dst_start = (i * (*safe_out_frame.ptr).linesize[0]) as usize;
                             ptr::copy_nonoverlapping(src_y[src_start..].as_ptr(), 
                                                     (*safe_out_frame.ptr).data[0].add(dst_start), 
                                                     w as usize);
                         }
                         
                         // Copy U
                         let src_u = &up_frame.data[y_size .. y_size+u_size];
                         for i in 0..h/2 {
                             let src_start = (i * w/2) as usize;
                             let dst_start = (i * (*safe_out_frame.ptr).linesize[1]) as usize;
                              ptr::copy_nonoverlapping(src_u[src_start..].as_ptr(), 
                                                     (*safe_out_frame.ptr).data[1].add(dst_start), 
                                                     (w/2) as usize);
                         }

                         // Copy V
                         let src_v = &up_frame.data[y_size+u_size .. y_size+u_size+v_size];
                         for i in 0..h/2 {
                             let src_start = (i * w/2) as usize;
                             let dst_start = (i * (*safe_out_frame.ptr).linesize[2]) as usize;
                              ptr::copy_nonoverlapping(src_v[src_start..].as_ptr(), 
                                                     (*safe_out_frame.ptr).data[2].add(dst_start), 
                                                     (w/2) as usize);
                         }
                    }
                    
                    (*safe_out_frame.ptr).pts = video_frames_done; 
                    video_frames_done += 1;
                    total_processed += 1;

                    if total_processed % 10 == 0 {
                         print!("\r🚀 Processing Frame: {}", total_processed);
                         std::io::stdout().flush().ok();
                    }

                    if ffi::avcodec_send_frame(safe_encode_ctx.ptr, safe_out_frame.ptr) >= 0 {
                        while ffi::avcodec_receive_packet(safe_encode_ctx.ptr, safe_pkt.ptr) == 0 {
                            ffi::av_packet_rescale_ts(safe_pkt.ptr, (*safe_encode_ctx.ptr).time_base, (*out_video_stream).time_base);
                            (*safe_pkt.ptr).stream_index = (*out_video_stream).index;
                            ffi::av_interleaved_write_frame(safe_out_ctx.ptr, safe_pkt.ptr);
                            ffi::av_packet_unref(safe_pkt.ptr);
                        }
                    }
                },
                Ok(EncoderMsg::Audio(packet_data)) => {
                    if !out_audio_stream.is_null() {
                         let mut safe_new_pkt = crate::video::wrappers::SafePacket::new();
                         ffi::av_new_packet(safe_new_pkt.ptr, packet_data.data.len() as i32);
                         ptr::copy_nonoverlapping(packet_data.data.as_ptr(), (*safe_new_pkt.ptr).data, packet_data.data.len());
                         (*safe_new_pkt.ptr).pts = packet_data.pts;
                         (*safe_new_pkt.ptr).dts = packet_data.dts;
                         (*safe_new_pkt.ptr).duration = packet_data.duration;
                         (*safe_new_pkt.ptr).flags = packet_data.flags;
                         
                         ffi::av_packet_rescale_ts(safe_new_pkt.ptr, input_audio_tb, (*out_audio_stream).time_base);
                         (*safe_new_pkt.ptr).stream_index = (*out_audio_stream).index;
                         ffi::av_interleaved_write_frame(safe_out_ctx.ptr, safe_new_pkt.ptr);
                    }
                },
                Ok(EncoderMsg::EOF) | Err(_) => {
                    // Flush
                    ffi::avcodec_send_frame(safe_encode_ctx.ptr, ptr::null());
                     while ffi::avcodec_receive_packet(safe_encode_ctx.ptr, safe_pkt.ptr) == 0 {
                            ffi::av_packet_rescale_ts(safe_pkt.ptr, (*safe_encode_ctx.ptr).time_base, (*out_video_stream).time_base);
                            (*safe_pkt.ptr).stream_index = (*out_video_stream).index;
                            ffi::av_interleaved_write_frame(safe_out_ctx.ptr, safe_pkt.ptr);
                            ffi::av_packet_unref(safe_pkt.ptr);
                     }
                    break;
                }
            }
        }

        ffi::av_write_trailer(safe_out_ctx.ptr);
        
        // Manual cleanup removed! Drop traits handle:
        // - safe_encode_ctx (avcodec_free_context)
        // - safe_out_ctx (avio_closep if needed + avformat_free_context)
        // - safe_out_frame
        // - safe_pkt
    }
    Ok(())
}
