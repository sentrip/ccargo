use crate::utils::paths;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct BinaryWriter(Vec<u8>);

impl BinaryWriter {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }
    
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional)
    }

    pub fn write_u8(&mut self, value: u8) {
        self.0.push(value)
    }

    pub fn write_u32(&mut self, value: u32) {
        self.0.extend(&value.to_le_bytes());
    }

    pub fn write_u64(&mut self, value: u64) {
        self.0.extend(&value.to_le_bytes());
    }
    
    pub fn write_bytes<B: AsRef<[u8]>>(&mut self, value: B) {
        let bytes = value.as_ref();
        self.write_u64(bytes.len() as u64);
        self.0.extend(bytes);
    }
    
    pub fn write_path<P: AsRef<Path>>(&mut self, value: P) {
        let path = value.as_ref();
        let bytes = paths::path2bytes(path).expect(&format!("Invalid path `{:?}`", path));
        self.write_bytes(bytes);
    }
}


pub struct BinaryReader<'a>(pub &'a [u8]);

impl<'a> BinaryReader<'a> {
    pub fn read_u8(&mut self) -> Option<u8> {
        let r = self.0[0];
        self.0 = &self.0[1..];
        Some(r)
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        let (ret, rest) = self.0.split_at(4);
        self.0 = rest;
        Some(u32::from_le_bytes(ret.try_into().unwrap()))
    }
    
    pub fn read_u64(&mut self) -> Option<u64> {
        let (ret, rest) = self.0.split_at(8);
        self.0 = rest;
        Some(u64::from_le_bytes(ret.try_into().unwrap()))
    }
    
    pub fn read_bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.read_u64()? as usize;
        let (ret, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(ret)
    }
    
    pub fn read_path(&mut self) -> Option<PathBuf> {
        let bytes = self.read_bytes()?;
        Some(paths::bytes2path(bytes)
            .expect(&format!("Invalid path `{:?}`", bytes)))
    }
}
