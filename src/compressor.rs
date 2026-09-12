use std::{io, time};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub struct CompressResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub original_size: u64,
    pub compressed_size: u64,
    pub time_spent: time::Duration,
}

pub enum CompressorError {
    Io(io::Error),
    FfmpegExecutionFailed(String),
}

impl From<io::Error> for CompressorError {
    fn from(err: io::Error) -> Self {
        CompressorError::Io(err)
    }
}

pub fn compress_file(input_path: &Path) -> Result<CompressResult, CompressorError> {
    let start_time = Instant::now();

    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Invalid file stem"))?;

    let extension = input_path
        .extension()
    .and_then(|s| s.to_str())
    .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Invalid file extension"))?;

    let new_filename = format!("{stem}_compressed.{extension}");
    let output_path = input_path.with_file_name(new_filename);

    let status = Command::new("ffmpeg")
        .arg("-i")
        .arg(input_path)
        .arg("-map")
        .arg("0")
        .arg("-c:v")
        .arg("av1_nvenc")
        .arg("-cq")
        .arg("40")
        .arg("-maxrate")
        .arg("4M")
        .arg("-bufsize")
        .arg("8M")
        .arg("-preset")
        .arg("p7")
        .arg("-c:a")
        .arg("copy")
        .arg(&output_path)
        .status()?;

    if !status.success() {
        return Err(CompressorError::FfmpegExecutionFailed(
            format!("ffmpeg exited with status {:?}", status.code())
        ));
    }

    let original_size = fs::metadata(input_path)?.len();
    let compressed_size = fs::metadata(&output_path)?.len();
    let time_spent = start_time.elapsed();

    Ok(CompressResult {
        input_path: input_path.to_path_buf(),
        output_path,
        original_size,
        compressed_size,
        time_spent
    })
}
