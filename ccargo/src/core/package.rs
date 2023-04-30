use crate::cc::{BinType, Options, Profile, Build, Output};
use crate::core::{TargetName, PackageId, SourceId, Step, Context, FingerprintState, fingerprint};
use crate::toml::{CCARGO_SHARED, CCARGO_EXPORT};
use crate::utils::{IResult, InternedString, MsgWriter, paths};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;


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
    
    pub fn deps(&self) -> PathBuf {
        self.target_dir.join("deps")
    }

    pub fn fingerprint(&self) -> PathBuf {
        self.target_dir.join(".fingerprint")
    }

    pub fn output_dir(&self, pkg: &PackageId) -> PathBuf {
        let mut path = self.deps();
        path.push(&pkg.unique_name());
        path

    }
}


/// PackageMap - in package with name A, which package does package name B refer to?
#[derive(Default)]
pub struct PackageMap{
    // Global package map
    packages: HashMap<PackageId, Package>,
    /// Ambiguous package names that require a PackageID to be resolved
    ambiguous: HashMap<InternedString, BTreeMap<PackageId, Package>>,
}

impl PackageMap {
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn get(&self, id: &PackageId) -> &Package {
        self.packages.get(id).unwrap()
    }

    pub fn maybe_named(&self, name: &str) -> Option<&Package> {
        self.ambiguous
            .get(name)
            .and_then(|m| {
                if m.len() == 1 {
                    Some(m.values().next().unwrap())
                } else {
                    None
                }
            })
    }
    
    pub fn named(&self, name: &str, src: &PackageId) -> &Package {
        self.ambiguous
            .get(name)
            .and_then(|m| m.get(src))
            .unwrap()
    }
    
    pub fn iter(&self) -> impl Iterator<Item=&Package> {
        self.packages.values()
    }
    
    pub fn iter_named(&self, name: &str) -> impl Iterator<Item=&Package> {
        self.ambiguous.get(name)
            .unwrap()
            .values()
            .rev()
    }

    pub fn from_packages(packages: &[Package]) -> Self {
        let mut map = Self::default();
        for pkg in packages {
            map.packages.insert(pkg.id, pkg.clone());
            map.ambiguous
                .entry(pkg.name())
                .or_default()
                .insert(pkg.id, pkg.clone());
        }
        for pkg in packages {            
            let mut found_pkg = None;
            for dep in pkg.dependencies.iter() {
                for dep_pkg in map.iter_named(&dep.name) {
                    if dep_pkg.id.source_id() == dep.source_id {
                        found_pkg = Some(dep_pkg.clone());
                        break;
                    }
                }                
            }
            if let Some(dep_pkg) = found_pkg {
                map.ambiguous
                    .entry(dep_pkg.id.name())
                    .or_default()
                    .insert(pkg.id, dep_pkg);
            }
        }
        map
    }
}


/// Package
#[derive(Debug, Clone)]
pub struct Package(Arc<PackageInner>);

#[derive(Debug)]
pub struct PackageInner {
    pub id: PackageId,
    pub targets: Vec<Target>,
    pub steps: Vec<Step>,
    pub dependencies: Vec<Dependency>,
    pub warnings: Vec<String>,
}

impl std::ops::Deref for Package {
    type Target = PackageInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Package {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::get_mut(&mut self.0)
            .expect("Cannot modify Package when multiple references are alive")
    }
}

impl Package {
    pub fn new(inner: PackageInner) -> Self {
        Self(Arc::new(inner))
    }

    pub fn name(&self) -> InternedString {
        self.id.name()
    }

    pub fn root(&self) -> &Path {
        self.id.root()
    }
}

impl Eq for Package {}

impl PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::hash::Hash for Package {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    Static,
    Shared,
    Bin,
    Test,
    Bench,
    Example,
}


/// Target
#[derive(Clone)]
pub struct Target(Arc<TargetInner>);

#[derive(Debug)]
pub struct TargetInner {
    pub name: InternedString,
    pub package: PackageId,    
    pub kind: TargetKind,
    pub sources: Vec<PathBuf>,
    pub options: Options,
    pub depends: Vec<PublicPrivate<TargetName>>,
    pub includes: Vec<PublicPrivate<PathBuf>>,
    pub defines: Vec<PublicPrivate<(String, Option<String>)>>,
    pub rpath: Option<PathBuf>,
    pub export_header: Option<PathBuf>,
}

impl std::ops::Deref for Target {
    type Target = TargetInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Target {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::get_mut(&mut self.0)
            .expect("Cannot modify Target when multiple references are alive")
    }
}

impl Target {
    pub fn new(inner: TargetInner) -> Self {
        Self(Arc::new(inner))
    }
    
    pub fn full_name(&self) -> TargetName {
        TargetName::new(self.package.name(), self.name)
    }

    pub fn stable_hash<'a>(&self, ws: &'a Path) -> TargetStableHash<'a> {
        TargetStableHash(self.clone(), ws)
    }
}

impl Eq for Target {}

impl PartialEq for Target {
    fn eq(&self, other: &Target) -> bool {
        std::ptr::eq(&*self.0, &*other.0)
    }
}

impl std::hash::Hash for Target {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        std::ptr::hash(&*self.0, hasher)
    }
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}


/// Dependency
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: InternedString,
    pub source_id: SourceId,
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



impl Target {
    pub fn output_name(&self, target_triple: &str) -> PathBuf {
        let mut path = PathBuf::from(&self.name);
        path.set_extension(BinType::from(self.kind).ext(target_triple));
        path
    }

    pub fn output_path(&self, layout: &Layout, target_triple: &str) -> PathBuf {
        let mut path = layout.output_dir(&self.package);
        path.push(self.output_name(target_triple));
        path
    }

    pub fn dep_info_path(&self, layout: &Layout) -> PathBuf {
        let mut path = layout.fingerprint();
        path.push(&self.package.unique_name());
        path.push(&self.name);
        path.set_extension("d");
        path
    }
    
    pub fn fingerprint_path(&self, layout: &Layout) -> PathBuf {
        self.dep_info_path(layout).with_extension("hash")
    }

    pub fn runtime_path(&self, layout: &Layout, target_triple: &str) -> Option<PathBuf> {
        if let Some(rpath) = &self.rpath {
            let mut p = layout.target();
            if rpath.as_os_str() != "." {
                p.push(rpath);
            }
            p.push(self.output_name(target_triple));
            Some(p)
        } else {
            None
        }
    }
    
    // TODO: Check target syntax

    pub fn compile<O: Write + 'static, E: Write + 'static>(
        &self,
        cx: &Context,
        state: &FingerprintState,
        stdout: MsgWriter<O>,
        stderr: MsgWriter<E>,
    ) -> IResult<Output> {
        // TODO: Better tracking of export header dirtinesss/generation
        if let Some(path) = &self.export_header {
            if !path.exists() {
                gen_export_header(&self.name, path)?;
            }
        }
        
        let src_dir = self.package.root();
        let out_dir = cx.layout.output_dir(&self.package);
        let deps = cx.target_deps.get(self).unwrap();
        
        // NOTE: To standardize between Windows/Unix, dynamic libraries are linked with rpath `.`
        //   On Windows, the executable is linked to the `.lib` file statically (which can be anywhere), 
        //   and the `.dll` just needs to be in the executable folder at runtime.
        //
        //   On Unix, the executable is linked to the `.so`/`.dylib` file relative to the exe path.
        //   Since libraries are compiled into a different folder than the executable, we need to
        //   manually tell the linker where the dynamic libraries will be at runtime (i.e. the exe folder, `.`).
        let libs = if cx.toolchain.target().contains("msvc") {
            deps.libs.clone()
        } else {
            deps.libs.iter().map(|x| x.file_stem().unwrap().into()).collect()
        };

        // TODO: Add support for key-value defines at for TOML and for CC::BUILD
        let mut options = self.options.clone();
        for (k, v) in deps.defines.iter() {
            if let Some(v) = v {
                options.defines.insert(format!("{k}={v}"));
            } else {
                options.defines.insert(k.clone());
            }
        }
        
        let mut b = Build::new(&self.name, self.kind.into(), cx.toolchain.clone());
        
        if state.files.is_empty() {
            b.skip_compile();
        } else {
            for path in self.sources.iter() {
                if !state.files.contains(path) {
                    b.skip_file(path);
                }
            }
        }

        let output = b
            .src_dir(src_dir)
            .out_dir(&out_dir)
            .options(options)
            .files(self.sources.iter().cloned())
            .includes(&deps.includes)
            .libraries(libs)
            .profile(cx.profile.clone())
            .stdout(stdout)
            .stderr(stderr)
            .log_compile()
            .compile()?;
        
        fingerprint::translate_dep_info(
            output.objs.iter().map(|x| &x.0), 
            self.package.root(), 
            &cx.layout.target(), 
            &self.dep_info_path(cx.layout),
        )?;

        Ok(output)
    }

}

impl From<TargetKind> for BinType {
    fn from(value: TargetKind) -> Self {
        use TargetKind::*;
        match value {
            Bin | Test | Bench | Example => Self::Exe,
            Static => Self::Static,
            Shared => Self::Shared,
        }
    }
}


pub struct TargetStableHash<'a>(Target, &'a Path);

impl<'a> std::hash::Hash for TargetStableHash<'a> {
    fn hash<S: std::hash::Hasher>(&self, state: &mut S) {
        self.0.name.hash(state);
        self.0.kind.hash(state);
        self.0.options.hash(state);
        self.0.depends.hash(state);
        self.0.defines.hash(state);
        self.0.rpath.hash(state);
        self.0.package.stable_hash(self.1).hash(state);
        self.0.export_header
            .as_ref()
            .map(|v| v.strip_prefix(self.1).unwrap_or(v))
            .hash(state);
        for v in self.0.sources.iter() {
            v.strip_prefix(self.1).unwrap().hash(state);
        }
        for v in self.0.includes.iter() {
            v.strip_prefix(self.1).unwrap_or(v).hash(state);
        }
    }
}


fn gen_export_header(name: &str, path: &Path) -> IResult<()> {
    paths::create_dir_all(path.parent().unwrap())?;
    paths::write(path, export_header(
        &name.to_uppercase(),
        "EXPORT_",
        CCARGO_SHARED,
        CCARGO_EXPORT,
    ))?;
    Ok(())
}


fn export_header(
    name: &str, 
    api: &str,
    shared: &str, 
    export: &str, 
) -> String {
    format!(r#"#ifdef {shared}{name}
    #if defined(_MSC_VER)
        #ifdef {export}{name}
            #define {api}{name} __declspec(dllexport)
        #else
            #define {api}{name} __declspec(dllimport)
        #endif
    #elif defined(__GNUC__) || defined(__clang__)
        #define {api}{name} __attribute__((visibility("default")))
    #else
        #define {api}{name}
    #endif
#else
    #define {api}{name}
#endif
"#)
}
