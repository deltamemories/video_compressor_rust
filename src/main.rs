extern crate core;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod compressor;
pub mod db;
pub mod finder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_config_path = dirs::config_dir()
        .ok_or("Can't get config folder path")?
        .join("video_compressor_rust");
    let db_path = app_config_path.join("database.db");
    fs::create_dir_all(&app_config_path)?;

    println!("database path:{:?}", &db_path);

    let database = db::Db::open(&db_path)?;

    let path = get_target_path()?;

    let videos_all = finder::find_mp4_files(&path)?;
    let videos = database.register_files(&videos_all)?;

    let mut suc_processed: Vec<&Path> = Vec::new();
    let mut failed_videos: Vec<&Path> = Vec::new();

    for (index, video) in videos.iter().enumerate() {
        let Ok(video_size) = fs::metadata(&video).map(|m| m.len()) else {
            println!("Can't fetch file size for #{}, skip", index);
            continue;
        };

        database.set_as_processing(video, Some(video_size))?;

        let original_modify_time = fs::metadata(video)?.modified();
        match original_modify_time {
            Ok(_) => {}
            Err(_) => {
                println!("Can't fetch modification time for #{}", index);
                continue;
            }
        }

        println!("Start compressing #{:?}, path {:?}", index, video);

        match compressor::compress_file(video) {
            Ok(result) => {
                let saved_bytes = result.original_size.saturating_sub(result.compressed_size);
                let compression_ratio = if result.original_size > 0 {
                    result.original_size as f64 / result.compressed_size as f64
                } else {
                    0.0
                };

                if original_modify_time.is_ok() {
                    let f_time = filetime::FileTime::from(original_modify_time?);
                    match filetime::set_file_mtime(&result.output_path, f_time) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("Can't update modification time for #{}", index);
                        }
                    }
                }

                database.set_as_completed(
                    video,
                    Some(result.original_size),
                    Some(result.compressed_size),
                )?;
                suc_processed.push(video);

                println!(
                    "Successfully compressed file #{}, Saved memory: {} MB, Compression ratio: {:.2}, Time spent: {}",
                    index,
                    saved_bytes / 1024 / 1024,
                    compression_ratio,
                    result.time_spent.as_secs()
                )
            }

            Err(err) => {
                database.set_as_failed(video, Some(&format!("{:?}", err)))?;
                failed_videos.push(video);
                eprintln!("Can't compress #{} due: {:?}", index, err)
            }
        }
    }

    if videos_all.len() != videos.len() {
        println!(
            "Successfully compressed {} of {}, skipped due duplicates: {}",
            suc_processed.len(),
            videos.len(),
            videos_all.len()
        );
    } else {
        println!(
            "Successfully compressed {} of {}",
            suc_processed.len(),
            videos.len()
        );
    }

    if failed_videos.len() > 0 {
        println!("Failed videos:");
        for f in failed_videos {
            println!("{:?}", f);
        }
    }

    Ok(())
}

fn get_target_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(arg) = std::env::args().nth(1) {
        return Ok(PathBuf::from(arg.trim()));
    }

    println!("Enter videos folder path");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().trim_end_matches('"');

    if trimmed.is_empty() {
        return Err("Empty path".into());
    }

    Ok(PathBuf::from(trimmed))
}
