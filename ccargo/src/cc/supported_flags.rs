use super::Tool;
use crate::core::Config;
use crate::utils::{IResult, hash_u64, BinaryWriter, BinaryReader};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;


/// Load the flags cache from disk
pub fn load(config: &Config) -> IResult<()> {
    let mut loaded = LOADED.lock().unwrap();
    if *loaded {
        return Ok(());
    }
    let path = flags_cache_file(config)?;
    if !path.exists() {
        *loaded = true;
        return Ok(());
    }
    let data = std::fs::read(path)?;
    *CACHE.lock().unwrap() = deserialize_cache(&data)
        .ok_or_else(|| anyhow::anyhow!("Failed to deserialize cache"))?;
    *loaded = true;
    Ok(())
}


/// Save the flags cache to disk
pub fn save(config: &Config) -> IResult<()> {
    // Save cache to disk
    let out = serialize_cache(&CACHE.lock().unwrap())?;
    std::fs::write(flags_cache_file(config)?, out)?;

    // Delete the build directory if it exists to remove unnecessary junk
    drop(std::fs::remove_dir_all(flags_cache_build_dir(config)?));
    Ok(())
}


/// Delete the flags cache directory
pub fn remove(config: &Config) -> IResult<()> {
    drop(std::fs::remove_dir_all(flags_cache_root(config)?));
    Ok(())
}


/// Check if a flag is supported by the given compiler
pub fn is_flag_supported(
    tool: &Tool,
    flag: &str,
    cpp: bool,
    config: &Config,
) -> IResult<bool> {
    // Ensure the cache has been loaded from disk if it exists
    load(config)?;

    // Check if the flag is in the cache, or insert new flags for the tool
    {
        let mut cache = CACHE.lock().unwrap();
        if let Some(flags) = cache.get(tool.path()) {
            if flags.contains(flag) {
                return Ok(true);
            }
        } else {
            cache.insert(tool.path().to_path_buf(), SupportedFlags::default());
        }
    }

    // Check if the flag is supported by the tool 
    // This might take long, so we don't hold the lock
    let supported = check_is_flag_supported(tool, flag, cpp, config)?;
    
    // Insert the flag into the cache
    CACHE
        .lock()
        .unwrap()
        .get_mut(tool.path())
        .unwrap()
        .insert(flag);

    Ok(supported)
}


// Runtime cache of supported flags
type Cache = HashMap<PathBuf, SupportedFlags>;

lazy_static::lazy_static! {
    static ref CACHE: Mutex<Cache> = Mutex::new(Cache::new());
    static ref LOADED: Mutex<bool> = Mutex::new(false);
}


// Path of the flags cache root directory
fn flags_cache_root(config: &Config) -> IResult<PathBuf> {
    Ok(config.home().join("flags_cache"))
}


// Path of the binary flags cache file
fn flags_cache_file(config: &Config) -> IResult<PathBuf> {
    Ok(flags_cache_root(config)?.join("flags.bin"))
}


// Path of the build directory used for checking flags
fn flags_cache_build_dir(config: &Config) -> IResult<PathBuf> {
    Ok(flags_cache_root(config)?.join("build"))
}


// Run the tool to check if a flag is supported
fn check_is_flag_supported(
    tool: &Tool, 
    flag: &str,
    cpp: bool,
    config: &Config,
) -> IResult<bool> {
    let (path, name) = ensure_flag_check_file(flag, cpp, config)?;
    let supported = tool.to_command()
        .arg(flag)
        .arg("-c")
        .arg(name)
        .current_dir(path.parent().unwrap())
        .output()?
        .stderr
        .is_empty();
    Ok(supported)
}


// Ensures the flag check file used for compilation exists and returns its path and name
fn ensure_flag_check_file(flag: &str, cpp: bool, config: &Config) -> IResult<(PathBuf, String)> {    
    let hash = hash_u64(&flag);
    let ext = if cpp { "cpp" } else { "c" };
    let fname = format!("{hash:016x}_flags_check.{ext}");
    let out_dir = flags_cache_build_dir(config)?;
    if !out_dir.exists() {
        std::fs::create_dir_all(&out_dir)?;
    }
    let path = out_dir.join(&fname);
    if !path.exists() {
        std::fs::write(&path, "int main(void) {{ return 0; }}")?;
    }
    Ok((path, fname))
}


// Helper struct for efficiently storing set of supported flags
#[derive(Default)]
struct SupportedFlags(HashSet<u64>);
impl SupportedFlags {
    fn contains(&self, flag: &str) -> bool {
        self.0.contains(&hash_u64(&flag))
    }
    fn insert(&mut self, flag: &str) {
        self.0.insert(hash_u64(&flag));
    }
    
    fn serialize(&self, w: &mut BinaryWriter) {
        w.reserve((self.0.len() + 1) * 8);
        w.write_u64(self.0.len() as u64);
        for v in self.0.iter() {
            w.write_u64(*v);
        }
    }

    fn deserialize(r: &mut BinaryReader) -> Option<Self> {
        let n = r.read_u64()?;
        let mut h = HashSet::with_capacity(n as usize);
        for _ in 0..n {
            h.insert(r.read_u64()?);
        }
        Some(Self(h))
    }
}

// Serialize cache to Vec<u8>
fn serialize_cache(cache: &Cache) -> IResult<Vec<u8>> {
    let mut w = BinaryWriter::with_capacity(8);
    w.write_u64(cache.len() as u64);
    for (k, v) in cache.iter() {
        w.write_path(k);
        v.serialize(&mut w);
    }
    Ok(w.into_inner())
}

// Deserialize cache from bytes
fn deserialize_cache(bytes: &[u8]) -> Option<Cache> {
    let mut r = BinaryReader(bytes);
    let n = r.read_u64()?;
    let mut c = Cache::with_capacity(n as usize);
    for _ in 0..n {
        let path = r.read_path()?;
        let flags = SupportedFlags::deserialize(&mut r)?;
        c.insert(path, flags);
    }
    Some(c)
}
