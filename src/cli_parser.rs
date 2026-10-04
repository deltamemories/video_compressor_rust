use std::io::Write;
use std::path::PathBuf;


pub enum AppMode {
    ProcessDirectory(PathBuf),
    Continue,
}


pub fn get_target_path() -> Result<AppMode, Box<dyn std::error::Error>> {
    if let Some(arg) = std::env::args().nth(1) {
        let trimmed = arg.trim();
        if trimmed == "continue" {
            return Ok(AppMode::Continue)
        }

        return Ok(AppMode::ProcessDirectory(PathBuf::from(trimmed)));
    }


    println!("Enter videos folder path");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().trim_end_matches('"');

    if trimmed.is_empty() {
        return Err("Empty path".into());
    }

    Ok(AppMode::ProcessDirectory(PathBuf::from(trimmed)))
}
