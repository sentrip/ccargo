use crate::utils::IResult;
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Context;
use filetime::FileTime;

/// Equivalent to [`std::fs::create_dir_all`] with better error messages.
pub fn create_dir_all(p: impl AsRef<Path>) -> IResult<()> {
    let path = p.as_ref();
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory `{}`", path.display()))
}

/// Returns the last modification time of a file.
pub fn mtime(p: impl AsRef<Path>) -> IResult<FileTime> {
    let path = p.as_ref();
    let meta = fs::metadata(path)
        .with_context(|| format!("failed to stat `{}`", path.display()))?;
    Ok(FileTime::from_last_modification_time(&meta))
}

/// Equivalent to [`std::fs::read`] with better error messages.
pub fn read_bytes(p: impl AsRef<Path>) -> IResult<Vec<u8>> {
    let path = p.as_ref();
    fs::read(path)
        .with_context(|| format!("failed to read file `{}`", path.display()))
}

/// Equivalent to [`std::fs::read_to_string`] with better error messages.
pub fn read_string(p: impl AsRef<Path>) -> IResult<String> {
    let path = p.as_ref();
    fs::read_to_string(path)
        .with_context(|| format!("failed to read file `{}`", path.display()))
}

/// Equivalent to [`std::fs::write`] with better error messages.
pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> IResult<()> {
    let path = path.as_ref();
    fs::write(path, contents.as_ref())
        .with_context(|| format!("failed to write `{}`", path.display()))
}

/// Path normalization - like `canonicalize` but without using std::fs
pub fn normalize(p: impl AsRef<Path>) -> PathBuf {
    let path = p.as_ref();
    let mut out = PathBuf::new();
    for component in path.components() {
        let comp = component.as_os_str();
        if comp == "." {
            continue;
        }
        else if comp == ".." {
            out.pop();
        }
        else {
            out.push(comp)
        }
    }
    out
}

/// Converts a path to UTF-8 bytes.
pub fn path2bytes(path: &Path) -> IResult<&[u8]> {
    #[cfg(unix)]
    {
        use std::os::unix::prelude::*;
        Ok(path.as_os_str().as_bytes())
    }
    #[cfg(windows)]
    {
        match path.as_os_str().to_str() {
            Some(s) => Ok(s.as_bytes()),
            None => Err(anyhow::format_err!(
                "invalid non-unicode path: {}",
                path.display()
            )),
        }
    }
}

/// Converts UTF-8 bytes to a path.
pub fn bytes2path(bytes: &[u8]) -> IResult<PathBuf> {
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::prelude::*;
        Ok(PathBuf::from(OsStr::from_bytes(bytes)))
    }
    #[cfg(windows)]
    {
        use std::str;
        match str::from_utf8(bytes) {
            Ok(s) => Ok(PathBuf::from(s)),
            Err(..) => Err(anyhow::format_err!("invalid non-unicode path")),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn normalize_() {
        assert_eq!(PathBuf::from("a/b/c"), normalize("a/b/c"));
        assert_eq!(PathBuf::from("a/b/c/e"), normalize("a/b/c/./e"));
        assert_eq!(PathBuf::from("a/b/e"), normalize("a/b/c/../e"));
        #[cfg(windows)]
        assert_eq!(PathBuf::from("a/b/e"), normalize("a\\b\\c\\..\\e"));
    }
}
