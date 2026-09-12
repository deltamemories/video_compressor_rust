extern crate core;

use std::path::Path;

pub mod finder;
pub mod compressor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("Y:\\0");

    let videos = finder::find_mp4_files(path)?;

    for (index, video) in videos.iter().enumerate() {
        println!("Start compressing #{:?}, path {:?}", index, video);

        match compressor::compress_file(video) {
            Ok(result) => {
                let saved_bytes = result.original_size.saturating_sub(result.compressed_size);
                let compression_ratio = if result.original_size > 0 {
                    result.original_size as f64 / result.compressed_size as f64
                } else {
                    0.0
                };

                println!(
                    "Successfully compressed file #{}, Saved memory: {} MB, Compression ratio: {}", index, saved_bytes/1024/1024, compression_ratio
                )
            }

            Err(_) => {
                eprintln!("Can't compress #{}", index)
            }
        }
    }

    Ok(())
}
