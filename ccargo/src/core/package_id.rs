use crate::utils::{IResult, InternedString, ToSemver, hash_u64};
use std::collections::HashSet;
use std::fmt;
use std::hash::{self, Hash};
use std::sync::Mutex;
use std::path::{Path, PathBuf};
use serde::{ser, de};
use anyhow::bail;


// Global caches
lazy_static::lazy_static! {
    static ref PACKAGE_ID_CACHE: Mutex<HashSet<&'static PackageIdInner>> = Default::default();
    static ref SOURCE_ID_CACHE: Mutex<HashSet<&'static SourceIdInner>> = Default::default();
}


/// Identifier for a specific version of a package in a specific source.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId {
    inner: &'static PackageIdInner,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PackageIdInner {
    name: InternedString,
    version: semver::Version,
    source_id: SourceId,
}

impl PackageId {
    pub fn new(
        name: impl Into<InternedString>,
        version: impl ToSemver,
        source_id: SourceId,
    ) -> IResult<PackageId> {
        let name = name.into();
        let version = version.to_semver()?;
        let inner = PackageIdInner {name, version, source_id};
        let mut cache = PACKAGE_ID_CACHE.lock().unwrap();
        let inner = cache.get(&inner).cloned().unwrap_or_else(|| {
            let inner = Box::leak(Box::new(inner));
            cache.insert(inner);
            inner
        });
        Ok(PackageId { inner })
    }
    
    pub fn name(self) -> InternedString {
        self.inner.name
    }

    pub fn version(self) -> &'static semver::Version {
        &self.inner.version
    }

    pub fn source_id(self) -> SourceId {
        self.inner.source_id
    }
        
    pub fn root(&self) -> &'static Path {
        self.source_id().path()
    }

    /// Returns a value that implements a "stable" hashable value.
    ///
    /// Stable hashing removes the path prefix of the workspace from path
    /// packages. This helps with reproducible builds, since this hash is part
    /// of the symbol metadata, and we don't want the absolute path where the
    /// build is performed to affect the binary output.
    pub fn stable_hash(self, workspace: &Path) -> PackageIdStableHash<'_> {
        PackageIdStableHash(self, workspace)
    }

    /// Returns a unique string that can be used to identify this package (e.g. in folder names)
    pub fn unique_name(self) -> String {
        let h = hash_u64(&(self.inner.name, self.inner.version.clone()));
        format!("{}-{:016x}", self.inner.name, h)
    }
}


/// Unique identifier for a source of packages.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId {
    inner: &'static SourceIdInner
}


#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SourceIdInner {
    path: PathBuf,
}

impl SourceId {
    pub fn new(path: PathBuf) -> SourceId {
        let inner = SourceIdInner { path };
        let mut cache = SOURCE_ID_CACHE.lock().unwrap();
        let inner = cache.get(&inner).cloned().unwrap_or_else(|| {
            let inner = Box::leak(Box::new(inner));
            cache.insert(inner);
            inner
        });
        SourceId { inner }
    }
    
    pub fn path(self) -> &'static Path {
        &self.inner.path
    }

    pub fn manifest_path(self) -> PathBuf {
        self.inner.path.join(crate::toml::CCARGO_TOML)
    }
    
    /// Hashes `self`.
    ///
    /// For paths, remove the workspace prefix so the same source will give the
    /// same hash in different locations.
    pub fn stable_hash<S: hash::Hasher>(self, workspace: &Path, into: &mut S) {
        if let Ok(p) = self
            .inner
            .path
            .strip_prefix(workspace)
        {
            p.to_str().unwrap().hash(into);
            return;
        }
        self.hash(into)
    }

    /// Parses a source URL and returns the corresponding ID.
    ///
    /// ## Example
    ///
    /// ```
    /// use ccargo::core::SourceId;
    /// SourceId::from_url("git+https://github.com/alexcrichton/\
    ///                     libssh2-static-sys#80e71a3021618eb05\
    ///                     656c58fb7c5ef5f12bc747f");
    /// ```
    pub fn from_url(string: &str) -> IResult<SourceId> {
        let mut parts = string.splitn(2, '+');
        let kind = parts.next().unwrap();
        let url = parts
            .next()
            .ok_or_else(|| anyhow::format_err!("invalid source `{}`", string))?;

        Ok(match kind {
            "path" => SourceId::new(url.into()),
            kind => bail!("unsupported source protocol: {}", kind),
        })
    }
}


pub struct PackageIdStableHash<'a>(PackageId, &'a Path);

impl<'a> Hash for PackageIdStableHash<'a> {
    fn hash<S: hash::Hasher>(&self, state: &mut S) {
        self.0.inner.name.hash(state);
        self.0.inner.version.hash(state);
        self.0.inner.source_id.stable_hash(self.1, state);
    }
}


impl fmt::Debug for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} v{}", self.inner.name, self.inner.version)?;
        Ok(())
    }
}

impl fmt::Debug for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "path+{}", self.inner.path.to_str().unwrap())?;
        Ok(())
    }
}


impl ser::Serialize for PackageId {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        s.collect_str(&format_args!(
            "{} {} ({})",
            self.inner.name,
            self.inner.version,
            self.inner.source_id
        ))
    }
}

impl<'de> de::Deserialize<'de> for PackageId {
    fn deserialize<D>(d: D) -> Result<PackageId, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        let mut s = string.splitn(3, ' ');
        let name = s.next().unwrap();
        let name = InternedString::new(name);
        let version = match s.next() {
            Some(s) => s,
            None => return Err(de::Error::custom("invalid serialized PackageId")),
        };
        let version = version.to_semver().map_err(de::Error::custom)?;
        let url = match s.next() {
            Some(s) => s,
            None => return Err(de::Error::custom("invalid serialized PackageId")),
        };
        let url = if url.starts_with('(') && url.ends_with(')') {
            &url[1..url.len() - 1]
        } else {
            return Err(de::Error::custom("invalid serialized PackageId"));
        };
        let source_id = SourceId::from_url(url).map_err(de::Error::custom)?;

        Ok(PackageId::new(name, version, source_id).unwrap())
    }
}

impl ser::Serialize for SourceId {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        s.collect_str(self)
    }
}

impl<'de> de::Deserialize<'de> for SourceId {
    fn deserialize<D>(d: D) -> Result<SourceId, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        SourceId::from_url(&string).map_err(de::Error::custom)
    }
}
