# X-Stream AI Video Upscaler

A high-performance, modular Rust library for AI video upscaling (1080p), utilizing `ffmpeg` for media handling and `onnxruntime` for AI inference.

## 🚀 Running on GitHub (Codespaces) or Linux

This project is optimized for Linux environments (GitHub Codespaces, Actions, or WSL).

### 1. Prerequisites (Ubuntu/Debian)
You need to install FFmpeg libraries and Clang for the build to succeed.

```bash
sudo apt-get update
sudo apt-get install -y clang libavcodec-dev libavformat-dev libavutil-dev libavfilter-dev libavdevice-dev libswresample-dev libswscale-dev
```

### 2. Setup
1.  **Model**: You need a `model.onnx` file in the root directory.
    *   *Note: Large files are git-ignored. You may need to download or upload your model manually.*
2.  **Input**: Place your `test_input.mp4` in the root.

### 3. Build & Run
```bash
# Debug run with logging
RUST_LOG=info cargo run -- --input test_input.mp4 --output output.mp4

# Release build (Faster)
cargo build --release
./target/release/x-stream --input test_input.mp4 --output output.mp4
```

## 📂 Project Structure
*   `src/lib.rs`: Library entry point.
*   `src/api.rs`: Public `Engine` API.
*   `src/video/`: Decoder, Encoder, and Safe FFI wrappers.
*   `src/ai/`: AI Processor logic.

## ⚠️ Windows Note
Building on Windows requires a specific setup of `ffmpeg` libraries in the path and `clang`. If you encounter `errno.h` errors, we highly recommend using **WSL2** or **GitHub Codespaces**.
