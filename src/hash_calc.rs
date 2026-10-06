use std::io;
use std::path::Path;

pub fn compute_hash<P: AsRef<Path>>(file: P) -> io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    
    hasher.update_mmap_rayon(file.as_ref())?;
    
    Ok(hasher.finalize())
}