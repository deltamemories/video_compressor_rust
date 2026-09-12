use std::fs;
use std::path::{Path, PathBuf};

pub fn find_mp4_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut mp4_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "mp4") {
            mp4_files.push(path);
        }
    }

    Ok(mp4_files)
}
