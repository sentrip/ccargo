pub use anyhow::Error;
pub type IResult<T> = anyhow::Result<T>;

mod byte_find;
pub use byte_find::ByteFind;

pub mod cached_mtime;

mod color_string;
pub use color_string::{Color, ColorString, WriteColorExt};

mod command_ext;
pub use command_ext::CommandExt;

mod graph;
pub use graph::{Graph, BitVec};

mod hasher;
pub use hasher::StableHasher;

mod interned_string;
pub use interned_string::InternedString;

pub mod lev_distance;

mod msg_queue;
pub use msg_queue::{MsgQueue, MsgWriter};

pub mod paths;

mod serde_bin;
pub use serde_bin::{BinaryReader, BinaryWriter, BinarySerialize, BinaryDeserialize};

mod shell;
pub use shell::{Shell, ColorChoice, Verbosity};

mod semver_ext;
pub use semver_ext::ToSemver;


pub fn ccargo_home() -> IResult<std::path::PathBuf> {
    if let Some(path) = home::home_dir() {
        Ok(path.join(".ccargo"))
    } else {
        anyhow::bail!("Failed to locate ccargo home directory")
    }
}

pub fn hash_u64<H: std::hash::Hash>(value: &H) -> u64 {
    let mut h = StableHasher::new();
    value.hash(&mut h);
    std::hash::Hasher::finish(&h)
}

pub fn to_hex(num: u64) -> String {
    const TABLE: &[u8] = b"0123456789abcdef";
    let mut b = Vec::new();
    for byte in num.to_le_bytes() {
        b.push(TABLE[(byte >> 4) as usize]);
        b.push(TABLE[(byte & 0xf) as usize]);
    }
    // SAFETY: Hex strings are always valid UTF-8
    unsafe { String::from_utf8_unchecked(b) }
}
