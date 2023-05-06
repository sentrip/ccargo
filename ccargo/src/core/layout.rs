use crate::cc::Profile;
use crate::core::PackageId;
use std::path::{Path, PathBuf};

/// Layout
pub struct Layout {
    root_dir: PathBuf, 
    target_dir: PathBuf,   
}

impl Layout {
    pub fn new<P: AsRef<Path>>(cwd: P, profile: &Profile, target: Option<&str>) -> Self {
        let root_dir = cwd.as_ref().to_path_buf();
        // EITHER
        //      `target/debug`
        // OR
        //      `target/x86_64-pc-windows-msvc/debug`
        let mut target_dir = root_dir.join("target");
        if let Some(t) = target {
            target_dir.push(t);
        }
        target_dir.push(profile.dir_name);
        Self { root_dir, target_dir }
    }

    pub fn root(&self) -> PathBuf {
        self.root_dir.clone()
    }

    pub fn target(&self) -> PathBuf {
        self.target_dir.clone()
    }
    
    pub fn bin(&self) -> PathBuf {
        self.target_dir.join("bin")
    }

    pub fn fingerprint(&self) -> PathBuf {
        self.target_dir.join(".fingerprint")
    }

    pub fn output_dir(&self, pkg: &PackageId) -> PathBuf {
        let mut path = self.bin();
        path.push(&pkg.unique_name());
        path

    }
}



/// Struct that stores information about whether certain
/// data is public or private
pub struct PublicPrivate<T>(T, bool);

impl<T> PublicPrivate<T> {
    pub fn public(value: T) -> Self {
        Self(value, true)
    }
    pub fn private(value: T) -> Self {
        Self(value, false)
    }
    pub fn is_public(&self) -> bool {
        self.1
    }
}

impl<T: Default> Default for PublicPrivate<T> {
    fn default() -> Self {
        Self::private(T::default())
    }
}

impl<T> std::ops::Deref for PublicPrivate<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for PublicPrivate<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Eq> Eq for PublicPrivate<T> {}

impl<T: Copy> Copy for PublicPrivate<T> {}

impl<T: Clone> Clone for PublicPrivate<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}

impl<T: PartialEq> PartialEq for PublicPrivate<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0) && self.1 == other.1
    }
}

impl<T: std::hash::Hash> std::hash::Hash for PublicPrivate<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for PublicPrivate<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(if self.is_public() { "Public" } else { "Private" })
            .field(&self.0)
            .finish()
    }
}

