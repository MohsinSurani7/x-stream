// src/ai/processor.rs

use onnxruntime::{environment::Environment, tensor::OrtOwnedTensor, session::Session};
use ndarray::{Array4, IxDyn};
use image::{ImageBuffer, Rgb, Luma, imageops};
use image::imageops::FilterType;
use anyhow::{Result, anyhow};
use std::io::Write;
use crate::video::types::RawFrame;

pub struct AIProcessor<'a> {
    pub session: Session<'a>,
}

impl<'a> AIProcessor<'a> {
    pub fn new(model_path: &'a str, env: &'a Environment) -> Result<Self> {
        let session = env.new_session_builder()?.with_model_from_file(model_path).map_err(|e| anyhow!("{:?}", e))?;
        Ok(Self { session })
    }

    pub fn process_frame_y(&mut self, frame: &RawFrame) -> Result<Vec<u8>> {
        let (tw, th) = (224, 224); // Model Input Size
        
        // 1. High Quality Downscaling of Input (RGB) -> 224x224
        // Use Image crate for this to avoid Aliasing from Nearest Neighbor
        let mut input_tensor_data = Vec::with_capacity((tw * th) as usize);
        
        if let Some(img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(frame.width as u32, frame.height as u32, frame.data.clone()) {
            let resized = imageops::resize(&img, tw as u32, th as u32, FilterType::CatmullRom); // Bicubic Downscale
            
            // Convert to Y-Channel Tensor (0-1)
            for pixel in resized.pixels() {
                let r = pixel[0] as f32;
                let g = pixel[1] as f32;
                let b = pixel[2] as f32;
                let y_val = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
                input_tensor_data.push(y_val);
            }
        } else {
             // Fallback to zeros if image load fails
             input_tensor_data = vec![0.0; (tw * th) as usize];
        }

        let tensor = Array4::from_shape_vec((1, 1, th as usize, tw as usize), input_tensor_data)?;
        let outputs: Vec<OrtOwnedTensor<f32, IxDyn>> = self.session.run(vec![tensor])?;
        let tensor_out = &outputs[0];
        let shape = tensor_out.shape();
        
        let mut pixels = Vec::with_capacity(shape[2] * shape[3]);
        
        // Output normalization check
        // User reports "faint" image. We implement Auto-Contrast (Min-Max Normalization).
        let mut min_v = f32::MAX;
        let mut max_v = f32::MIN;
        
        // Pass 1: Find Range
        for val in tensor_out.iter() {
            if *val < min_v { min_v = *val; }
            if *val > max_v { max_v = *val; }
        }
        
        // Avoid division by zero
        if max_v - min_v < 0.00001 {
            max_v = min_v + 1.0; 
        }

        // Pass 2: Normalize to 0-255
        for y in 0..shape[2] {
            for x in 0..shape[3] {
                let v = tensor_out[[0, 0, y, x]];
                // Rescale v from [min_v, max_v] to [0.0, 255.0]
                let normalized = (v - min_v) / (max_v - min_v); 
                let pixel = (normalized * 255.0).clamp(0.0, 255.0) as u8;
                pixels.push(pixel);
            }
        }
        Ok(pixels)
    }
}

// --- SCALING LOGIC (High Quality) ---

// Upsaling 1-channel Grayscale (Y-Plane) using Cubic Interpolation
pub fn upscale_grayscale(src: &[u8], sw: i32, sh: i32, dw: i32, dh: i32) -> Vec<u8> {
    // 1. Create ImageBuffer from src
    if let Some(img) = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(sw as u32, sh as u32, src.to_vec()) {
        // 2. Resize
        let resized = imageops::resize(&img, dw as u32, dh as u32, FilterType::CatmullRom);
        // 3. Return raw bytes
        return resized.into_raw();
    }
    // Fallback (should not happen)
    vec![0u8; (dw * dh) as usize]
}

// Resizing RGB using Cubic Interpolation
pub fn upscale_to_original(rgb: &[u8], src_w: i32, src_h: i32, target_w: i32, target_h: i32) -> Vec<u8> {
    if let Some(img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(src_w as u32, src_h as u32, rgb.to_vec()) {
        let resized = imageops::resize(&img, target_w as u32, target_h as u32, FilterType::CatmullRom);
        return resized.into_raw();
    }
    vec![0u8; (target_w * target_h * 3) as usize]
}

// Simple RGB to YUV420P Converter
pub fn rgb_to_yuv420p(rgb: &[u8], w: i32, h: i32) -> Vec<u8> {
    let y_size = (w * h) as usize;
    let uv_size = (w / 2 * h / 2) as usize;
    let mut yuv = vec![0u8; y_size + uv_size * 2];
    
    // Y Plane
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let r = rgb[idx*3] as f32;
            let g = rgb[idx*3+1] as f32;
            let b = rgb[idx*3+2] as f32;
            
            yuv[idx] = (0.299*r + 0.587*g + 0.114*b).clamp(0.0, 255.0) as u8;
        }
    }
    
    // U and V Planes (Subsampled)
    for y in 0..h/2 {
        for x in 0..w/2 {
            let src_x = x * 2;
            let src_y = y * 2;
            let idx = ((src_y * w + src_x) * 3) as usize;
             let r = rgb[idx] as f32;
            let g = rgb[idx+1] as f32;
            let b = rgb[idx+2] as f32;
            
            let u = (-0.14713 * r - 0.28886 * g + 0.436 * b + 128.0).clamp(0.0, 255.0) as u8;
            let v = (0.615 * r - 0.51499 * g - 0.10001 * b + 128.0).clamp(0.0, 255.0) as u8;
            
            yuv[y_size + (y as usize * (w/2) as usize + x as usize)] = u;
            yuv[y_size + uv_size + (y as usize * (w/2) as usize + x as usize)] = v;
        }
    }
    yuv
}

// --- DEBUG HELPER ---
pub fn save_ppm(filename: &str, data: &[u8], width: i32, height: i32) -> std::io::Result<()> {
    let mut file = std::fs::File::create(filename)?;
    write!(file, "P6\n{} {}\n255\n", width, height)?;
    file.write_all(data)?;
    Ok(())
}
