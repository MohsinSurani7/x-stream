// src/api.rs

use std::sync::Arc;
use tokio::task;
use crossbeam_channel::bounded;
use onnxruntime::environment::Environment;
use anyhow::{Result, anyhow};
use crate::video::decoder;
use crate::video::encoder::{self, EncoderConfig};
use crate::video::types::{DecoderMsg, EncoderMsg};
use crate::ai::processor::{AIProcessor, upscale_grayscale, upscale_to_original, rgb_to_yuv420p, save_ppm};
use rsmpeg::ffi;
use std::ffi::CString;
use std::ptr;

pub struct Config {
    pub input_path: String,
    pub output_path: String,
    pub model_path: String,
    pub target_resolution: (u32, u32), // e.g. (1920, 1080)
}

pub struct Engine {
    config: Config,
    env: Arc<Environment>,
}

impl Engine {
    pub fn new(config: Config) -> Result<Self> {
        let env = Arc::new(Environment::builder()
            .with_name("x_stream_env")
            .with_log_level(onnxruntime::LoggingLevel::Warning)
            .build()?);
        Ok(Self { config, env })
    }

    pub async fn run(&self) -> Result<()> {
        println!("🚀 X-Stream Engine Starting...");
        println!("📂 Input: {}", self.config.input_path);
        println!("📂 Output: {}", self.config.output_path);

        let input = self.config.input_path.clone();
        let output = self.config.output_path.clone();
        let model = self.config.model_path.clone();
        let env = self.env.clone();
        let (tw, th) = self.config.target_resolution;

        // --- GET METADATA FOR ENCODER SETUP ---
        let (width, height, time_base) = unsafe {
            let in_c = CString::new(input.clone()).unwrap();
            let mut in_ctx = ptr::null_mut();
            if ffi::avformat_open_input(&mut in_ctx, in_c.as_ptr(), ptr::null_mut(), ptr::null_mut()) < 0 {
                return Err(anyhow!("Failed to probe input config"));
            }
            ffi::avformat_find_stream_info(in_ctx, ptr::null_mut());
             let mut video_stream_idx = -1;
            for i in 0..(*in_ctx).nb_streams {
                // Fixed dereference error here
                if (*(*(*(*in_ctx).streams.add(i as usize))).codecpar).codec_type == ffi::AVMEDIA_TYPE_VIDEO {
                    video_stream_idx = i as i32;
                    break;
                }
            }
            if video_stream_idx == -1 { return Err(anyhow!("No video stream")); }
            let s = *(*in_ctx).streams.add(video_stream_idx as usize);
            let w = (*(*s).codecpar).width;
            let h = (*(*s).codecpar).height;
            let tb = (*s).avg_frame_rate; // Avg fps
            ffi::avformat_close_input(&mut in_ctx);
            (w, h, ffi::av_inv_q(tb)) 
        };
        
        let (tx_video_raw, rx_video_raw) = bounded::<DecoderMsg>(5); 
        let (tx_encoder, rx_encoder) = bounded::<EncoderMsg>(5);
        let tx_encoder_audio = tx_encoder.clone();

        // --- THREAD 1: DECODER ---
        let input_dec = input.clone();
        let decoder_handle = std::thread::spawn(move || {
            decoder::run_decoder(&input_dec, tx_video_raw, tx_encoder_audio)
        });

        // --- THREAD 2: ENCODER ---
        let output_enc = output.clone();
        let input_enc = input.clone();
        let enc_config = EncoderConfig {
            width: tw as i32,
            height: th as i32,
            time_base: time_base,
            bitrate: 4_000_000,
        };
        let encoder_handle = std::thread::spawn(move || {
            encoder::run_encoder(&input_enc, &output_enc, rx_encoder, enc_config)
        });

        // --- THREAD 3: AI ---
        let ai_model = model.clone();
        let ai_env = env.clone();
        let ai_handle = std::thread::spawn(move || {
             let mut ai = AIProcessor::new(&ai_model, &ai_env).expect("Failed to init AI");
             
             for msg in rx_video_raw {
                match msg {
                    DecoderMsg::Video(raw) => {
                         if let Ok(ai_y_pixels) = ai.process_frame_y(&raw) {
                             let ai_y_upscaled = upscale_grayscale(&ai_y_pixels, 224, 224, tw as i32, th as i32);
                             let raw_upscaled_rgb = upscale_to_original(&raw.data, raw.width, raw.height, tw as i32, th as i32);
                             let mut yuv_data = rgb_to_yuv420p(&raw_upscaled_rgb, tw as i32, th as i32);
                             let y_size = (tw * th) as usize;
                             if ai_y_upscaled.len() == y_size && yuv_data.len() >= y_size {
                                 yuv_data[0..y_size].copy_from_slice(&ai_y_upscaled);
                             }

                             if raw.pts == 0 {
                                 let mut debug_rgb = Vec::with_capacity((tw * th * 3) as usize);
                                 for &y_pixel in &yuv_data[0..y_size] {
                                     debug_rgb.push(y_pixel);
                                     debug_rgb.push(y_pixel);
                                     debug_rgb.push(y_pixel);
                                 }
                                 let _ = save_ppm("debug_luma.ppm", &debug_rgb, tw as i32, th as i32);
                             }

                             let up_frame = crate::video::types::UpscaledFrame {
                                 data: yuv_data,
                                 width: tw as i32,
                                 height: th as i32,
                                 pts: raw.pts,
                             };
                             tx_encoder.send(EncoderMsg::Video(up_frame)).unwrap();
                         }
                    },
                    DecoderMsg::Audio(_) => {},
                    DecoderMsg::EOF => break,
                }
             }
             tx_encoder.send(EncoderMsg::EOF).unwrap();
        });

        task::spawn_blocking(move || {
            decoder_handle.join().unwrap().unwrap();
            ai_handle.join().unwrap();
            encoder_handle.join().unwrap().unwrap();
        }).await?;

        println!("\n✨ Engine Finished Successfully.");
        Ok(())
    }
}
