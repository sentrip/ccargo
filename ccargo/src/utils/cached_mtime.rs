use crate::utils::{paths, IResult};

use filetime::FileTime;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;


lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<PathBuf, FileTime>> = Mutex::new(HashMap::new());
}

pub fn clear() {
    CACHE.lock().unwrap().clear();
}

pub fn mtime(p: impl AsRef<Path>) -> IResult<FileTime> {
    let path = p.as_ref();
    if let Some(time) = CACHE.lock().unwrap().get(path) {
        return Ok(time.clone());
    }
    let mtime = paths::mtime(path)?;
    CACHE.lock().unwrap().insert(path.to_owned(), mtime);
    Ok(mtime)
}
