use std::path::{Path, PathBuf};
use std::{fs, time};
use crate::db::AddFileError;

pub mod cli_parser;
pub mod compressor;
pub mod db;
pub mod finder;
pub mod hash_calc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_config_path = dirs::config_dir()
        .ok_or("Can't get config folder path")?
        .join("video_compressor_rust");

    let db_path = app_config_path.join("database.db");
    fs::create_dir_all(&app_config_path)?;
    println!("database path:{:?}", &db_path);

    let mut database = db::Db::open(&db_path)?;

    let app_mode = cli_parser::get_target_path()?;

    let videos_to_compress: Vec<PathBuf> = match app_mode {
        cli_parser::AppMode::Continue => {
            eprintln!("Continue mode not implemented yet");
            return Ok(())
        }

        cli_parser::AppMode::ProcessDirectory(dir) => {
            finder::find_mp4_files(&dir)?
        }
    };

    let mut suc_processed: Vec<&Path> = Vec::new();
    let mut failed_videos: Vec<&Path> = Vec::new();

    let mut saved_bytes_total: u64 = 0;
    let mut time_spent_total = time::Duration::from_secs(0);

    println!("Will be processed: {} files", videos_to_compress.len());

    for (index, video) in videos_to_compress.iter().enumerate() {
        let start_time = time::Instant::now();
        println!("Start processing #{:?}, path {:?}", index, video);

        let Ok(video_size) = fs::metadata(&video).map(|m| m.len() as i64) else {
            failed_videos.push(video);
            eprintln!("Can't fetch file size for #{}, skip", index);
            continue;
        };

        let Ok(video_original_hash) = hash_calc::compute_hash(&video).map(|h| h.to_string()) else {
            failed_videos.push(video);
            eprintln!("Can't compute hash for #{}, skip", index);
            continue;
        };

        let is_compressed_output = database.is_compressed_output(&video_original_hash)?;

        if is_compressed_output {
            println!("#{:?} is compressed output, skip", index);
            continue;
        }

        let video_path = match video.to_str() {
            Some(path) => path.to_owned().replace("\\", "/"),
            None => {
                failed_videos.push(video);
                eprintln!("Can't convert video PathBuf to String");
                continue;
            }
        };

        let video_id = match database.add_file(db::FileInfo{
            path: video_path,
            hash: video_original_hash,
            size: video_size,
        }) {
            Ok(id) => id,

            Err(AddFileError::AlreadyExists) => {
                println!("#{:?} already exists in database, skip", index);
                continue;
            }

            Err(AddFileError::DatabaseError(err)) => {
                return Err(err.into())
            }
        };

        let job_id = database.start_job(video_id)?;


        let Ok(original_modify_time) = fs::metadata(video)
            .and_then(|m| m.modified())
        else {
            failed_videos.push(video);
            eprintln!("Can't fetch modification time for #{}", index);
            let elapsed = start_time.elapsed().as_secs() as i64;
            database.fail_job(job_id, "Can't fetch modification time", elapsed)?;
            continue;
        };

        match compressor::compress_file(video) {
            Ok(result) => {
                let saved_bytes = result.original_size.saturating_sub(result.compressed_size);
                let compression_ratio = if result.compressed_size > 0 {
                    result.original_size as f64 / result.compressed_size as f64
                } else {
                    0.0
                };

                let f_time = filetime::FileTime::from(original_modify_time);
                match filetime::set_file_mtime(&result.output_path, f_time) {
                    Ok(_) => {}
                    Err(_) => {
                        eprintln!("Can't update modification time for #{}", index);
                        let elapsed = start_time.elapsed().as_secs() as i64;
                        database.fail_job(job_id, "Can't update modification time", elapsed)?;
                        continue;
                    }
                }

                let Ok(video_compressed_hash) = hash_calc::compute_hash(&result.output_path).map(|h| h.to_string()) else {
                    failed_videos.push(video);
                    eprintln!("Can't compute compressed file hash for #{}, skip", index);
                    let elapsed = start_time.elapsed().as_secs() as i64;
                    database.fail_job(job_id, "Can't compute hash for compressed file", elapsed)?;
                    continue;
                };

                let compressed_video_path = match result.output_path.to_str() {
                  Some(path) => path.to_owned().replace("\\", "/"),
                    None => {
                        failed_videos.push(video);
                        eprintln!("Can't convert compressed video PathBuf to String");
                        continue;
                    }
                };

                let compressed_id = match database.add_file(db::FileInfo{
                    path: compressed_video_path,
                    hash: video_compressed_hash,
                    size: result.compressed_size as i64
                }) {
                    Ok(id) => id,
                    Err(AddFileError::AlreadyExists) => {
                        failed_videos.push(video);
                        eprintln!("Compressed output for #{:?} already exists in database", index);

                        let elapsed = start_time.elapsed().as_secs() as i64;
                        database.fail_job(
                            job_id,
                            "Compressed output already exists in database",
                            elapsed
                        )?;
                        continue;
                    }
                    Err(AddFileError::DatabaseError(err)) => {
                        return Err(err.into())
                    }
                } ;

                database.complete_job(
                    job_id,
                    compressed_id,
                    result.original_size as i64,
                    result.compressed_size as i64,
                    result.time_spent.as_secs() as i64,
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
                let elapsed = start_time.elapsed().as_secs() as i64;
                database.fail_job(job_id, &format!("Compressor error: {:?}", err), elapsed)?;
                failed_videos.push(video);
                eprintln!("Can't compress #{} due: {:?}", index, err)
            }
        }
    }

    println!(
        "Successfully compressed {} of {}",
        suc_processed.len(),
        videos_to_compress.len()
    );


    println!(
        "Total saved memory: {} MB, Total time spent: {}s",
        saved_bytes_total / 1024 / 1024,
        time_spent_total.as_secs()
    );

    if failed_videos.len() > 0 {
        eprintln!("Failed videos:");
        for f in failed_videos {
            eprintln!("{:?}", f);
        }
    }

    Ok(())
}
