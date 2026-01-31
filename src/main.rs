use clap::Parser;
use x_stream::{Engine, Config};
use anyhow::Result;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to input video file
    #[arg(short, long, default_value = "test_input.mp4")]
    input: String,

    /// Path to output video file
    #[arg(short, long, default_value = "output_refactored.mp4")]
    output: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Check if input file exists
    if !std::path::Path::new(&args.input).exists() {
        eprintln!("❌ Error: Input file '{}' not found.", args.input);
        return Ok(());
    }

    let config = Config {
        input_path: args.input,
        output_path: args.output,
        model_path: "model.onnx".to_string(),
        target_resolution: (1920, 1080),
    };

    let engine = Engine::new(config)?;
    engine.run().await?;

    Ok(())
}