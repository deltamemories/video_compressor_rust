extern crate core;

use std::path::{Path, PathBuf};
use std::{fs, time};

pub mod cli_parser;
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

    let app_mode = cli_parser::get_target_path()?;

    let (videos, videos_all): (Vec<PathBuf>, Vec<PathBuf>) = match app_mode {
        cli_parser::AppMode::Continue => {
            let pending_videos = database.get_pending()?;
            (pending_videos.clone(), pending_videos)
        }

        cli_parser::AppMode::ProcessDirectory(dir) => {
            let videos_all = finder::find_mp4_files(&dir)?;
            (database.register_files(&videos_all)?, videos_all)
        }
    };

    let mut suc_processed: Vec<&Path> = Vec::new();
    let mut failed_videos: Vec<&Path> = Vec::new();
    let mut saved_bytes_total: u64 = 0;
    let mut time_spent_total = time::Duration::from_secs(0);

    println!("Will be compressed: {} files", videos.len());

    for (index, video) in videos.iter().enumerate() {
        println!("Start compressing #{:?}, path {:?}", index, video);

        let Ok(video_size) = fs::metadata(&video).map(|m| m.len()) else {
            println!("Can't fetch file size for #{}, skip", index);
            database.set_as_failed(video, Some(&"Can't fetch file size".to_string()))?;
            continue;
        };

        database.set_as_processing(video, Some(video_size))?;

        let original_modify_time = fs::metadata(video)?.modified();
        match original_modify_time {
            Ok(_) => {}
            Err(_) => {
                println!("Can't fetch modification time for #{}", index);
                database.set_as_failed(video, Some(&"Can't fetch modification time".to_string()))?;
                continue;
            }
        }

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
                saved_bytes_total += result.original_size - result.compressed_size;
                time_spent_total += result.time_spent;

                println!(
                    "Successfully compressed file #{}, Saved memory: {} MB ({} -> {}), Compression ratio: {:.2}, Time spent: {}s",
                    index,
                    saved_bytes / 1024 / 1024,
                    result.original_size / 1024 / 1024,
                    result.compressed_size / 1024 / 1024,
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

    println!(
        "Total saved memory: {} MB, Total time spent: {}s",
        saved_bytes_total / 1024 / 1024,
        time_spent_total.as_secs()
    );

    if failed_videos.len() > 0 {
        println!("Failed videos:");
        for f in failed_videos {
            println!("{:?}", f);
        }
    }

    Ok(())
}
