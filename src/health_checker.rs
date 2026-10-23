use std::error::Error;
use std::process::{Command, Stdio};

pub fn check_health(input_path: String) -> Result<bool, Box<dyn Error>> {
    let status = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(input_path)
        .arg("-err_detect")
        .arg("explode")
        .arg("-xerror")
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    Ok(status.success())
}
