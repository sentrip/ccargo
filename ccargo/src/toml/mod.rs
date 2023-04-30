use crate::cc::*;
use crate::core::*;
use crate::utils::{paths, IResult, InternedString};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use semver::Version;
use anyhow::bail;

// TODO: Validate field names are kebab case
// TODO: Validate local/external dependencies

pub const CCARGO_TOML: &str = "CCargo.toml";
// TODO: Allow user to configure `CCARGO_SHARED_`/`CCARGO_EXPORT_` names?
pub const CCARGO_SHARED: &str = "CCARGO_SHARED_";
pub const CCARGO_EXPORT: &str = "CCARGO_EXPORT_";


pub fn read_package(
    path: &Path,
    config: &Config,
) -> IResult<Package> {
    let contents = paths::read_string(path)?;

    let toml: toml::Value = contents.parse()
        .map_err(|e| anyhow::Error::from(e).context("could not parse input as TOML"))?;

    let mut unused = BTreeSet::new();
    let manifest: TomlManifest = serde_ignored::deserialize(toml, |path| {
        let mut key = String::new();
        stringify(&mut key, &path);
        unused.insert(key);
    })?;

    let mut package = manifest.to_real(
        path.parent().unwrap(), 
        config,
    )?;
    
    for key in unused {
        package.warnings.push(format!("unused manifest key: `{}`", key));
        if key == "profiles.debug" {
            package.warnings.push("use `[profile.dev]` to configure debug builds".to_string());
        }
    }

    return Ok(package);
    
    fn stringify(dst: &mut String, path: &serde_ignored::Path<'_>) {
        use serde_ignored::Path;

        match *path {
            Path::Root => {}
            Path::Seq { parent, index } => {
                stringify(dst, parent);
                if !dst.is_empty() {
                    dst.push('.');
                }
                dst.push_str(&index.to_string());
            }
            Path::Map { parent, ref key } => {
                stringify(dst, parent);
                if !dst.is_empty() {
                    dst.push('.');
                }
                dst.push_str(key);
            }
            Path::Some { parent }
            | Path::NewtypeVariant { parent }
            | Path::NewtypeStruct { parent } => stringify(dst, parent),
        }
    }
}


/// This type is used to deserialize `CCargo.toml` files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlManifest {
    package: Option<TomlPackage>,
    lib: Option<Vec<TomlTarget>>,
    bin: Option<Vec<TomlTarget>>,
    dependencies: Option<BTreeMap<String, TomlDependency>>,
}


/// Represents the `package` section of a `CCargo.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlPackage {
    name: InternedString,
    version: Version,
    // package-wide options
    options: Option<TomlOptions>,
    // package-wide compiler defines
    define: Option<BTreeSet<String>>,
    // package-wide includes (relative to .toml file)
    include: Option<Vec<PathBuf>>,
}


// Represents an entry in the `dependencies` section of a `CCargo.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TomlDependency {
    /// In the simple format, only a version is specified, eg.
    /// `package = "<version>"`
    Simple(String),
    /// The simple format is equivalent to a detailed dependency
    /// specifying only a version, eg.
    /// `package = { version = "<version>" }`
    Detailed(TomlDetailedDependency),
}


// Detailed dependency information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlDetailedDependency {
    version: Option<String>,
    // `path` is relative to the file it appears in. If that's a `CCargo.toml`, it'll be relative to
    // that TOML file, and if it's a `.ccargo/config.toml` file, it'll be relative to that file.
    path: Option<String>,
}


/// Represents a target (lib/bin) section of a `CCargo.toml`
/// 
/// [[lib]]     -> library
/// [[bin]]     -> binary executable
/// [[test]]    -> test executable
/// [[bench]]   -> benchmark executable
/// [[example]] -> example executable
/// 
/// Link behaviour
/// lib                    -> headers, static linking
/// lib  (shared=true)     -> headers, dynamic linking
/// lib  (runtime=false)   -> headers, dynamic linking
/// lib  (runtime=true)    -> headers only (manual runtime loading using dlopen,...)
/// 
/// `depends_private` applies the above rules to this target only
/// 
/// `depends_public` is like `depends_private`, but additionally 
///  propagates headers/libs to targets that depend on this target
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlTarget {
    // name of the target (must be unique per package)
    name: InternedString,
    // sources (.c/.cpp/...) files (relative to .toml file)
    sources: Option<Vec<PathBuf>>,
    // public/private compiler defines (i.e. -DMYDEFINE)
    //  - private defines are only applied to the target they are defined for
    //  - public defines are propagated to all dependencies in the public subgraph
    // public/private `include`/`depends` are propagated in the same way
    define_public: Option<BTreeSet<String>>,
    define_private: Option<BTreeSet<String>>,
    // public/private include directories (relative to .toml file)
    include_public: Option<Vec<PathBuf>>,
    include_private: Option<Vec<PathBuf>>,
    // public/private dependencies required to build this target
    depends_public: Option<Vec<TomlTargetDependency>>,
    depends_private: Option<Vec<TomlTargetDependency>>,
    // options that control the compilation
    options: Option<TomlOptions>,
    // path to export header that defines export macros to be used by shared library functions
    export_header: Option<PathBuf>,

    // [[lib]] only options
    // is the target a shared library?
    shared: Option<bool>,
    // controls the linking behaviour and output location of dynamic libraries
    // w.r.t. the final executable (ignored for static libs, default is `.`)
    // NOTE: runtime = Some(...) implies shared = true
    //  `None` or `Some(false)`             -> headers + link
    //  `Some(true)` or `Some("path/lib")`  -> headers only
    runtime: Option<StringOrBool>,
}


/// Represents an entry in a target's `depends_public`/`depends_private` array in a `CCargo.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TomlTargetDependency {
    // dependency external to current project, e.g. `libexternal::target`
    External(TargetName),
    // dependency local to current project, e.g. `my_local_target`
    Local(InternedString),
}


// Represents an `options` entry for a package/target in a `CCargo.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlOptions {
    // C/C++
    language: Option<Language>,    
    // C/C++ standards
    std: Option<TomlStd>,
    // compiler/linker warning configuration
    warnings: Option<TomlWarnings>,
    // C runtime libraries used for linking
    //      None        -> cc::Crt::Default
    //      Some(true)  -> cc::Crt::Static
    //      Some(false) -> cc::Crt::Shared
    static_crt: Option<bool>,
    // flags passed to the compiler
    cc_flags: Option<BTreeSet<String>>,
    // flags passed to the linker
    ld_flags: Option<BTreeSet<String>>,
    // flags passed to the archiver
    ar_flags: Option<BTreeSet<String>>,
    // flags used when compiling assembly files
    asm_flags: Option<BTreeSet<String>>,
    // flags for unix-like targets
    unix: Option<TomlUnixFlags>,
}


// Flags that control the C/C++ standards/libraries used
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlStd {
    c: Option<StdC>,
    cxx: Option<StdCxx>,
    cxx_stdlib: Option<String>,
    gnu: Option<bool>,
}


// Flags that control how compiler warnings are emitted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TomlWarnings {
    // warning level - controls which warnings are displayed
    level: Option<WarningLevel>,
    // treat warnings as errors
    errors: Option<bool>,
    // extra platform-specific warning flags
    extra: Option<BTreeSet<String>>,
}


// Flags that are only supported on unix-like targets
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TomlUnixFlags {
    // position independent code
    pic: Option<bool>,
    // procedure linkage table
    plt: Option<bool>,
    // force emit frame pointer instructions
    force_frame_pointer: Option<bool>,
}


/// A StringOrBool can be parsed from either a TOML string or boolean
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, expecting = "expected a boolean or a string")]
pub enum StringOrBool {
    String(String),
    Bool(bool),
}
impl std::fmt::Display for StringOrBool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => s.fmt(f),
            Self::Bool(b) => b.fmt(f),
        }
    }
}


impl TomlManifest {
    fn to_real(
        &self,
        root: &Path,
        _config: &Config,
    ) -> IResult<Package> {
        let warnings = Vec::new();

        let package = match &self.package {
            Some(pkg) => pkg.clone(),
            None => bail!("no `package` section found"),
        };

        let package_name = package.name.trim();
        if package_name.is_empty() {
            bail!("package name cannot be an empty string")
        }
        // validate_package_name(package_name, "package name", "")?;
        
        let source_id = SourceId::new(root.to_path_buf());
        let id = PackageId::new(package_name, package.version.clone(), source_id)?;

        let mut targets = Vec::new();
        if let Some(lib) = &self.lib {
            for target in lib {
                // validate_target(target, "lib", "library");
                targets.push(target.to_real(root, id, "lib")?);
            }
        }
        if let Some(bin) = &self.bin {
            for target in bin {
                // validate_target(target, "bin", "binary");
                targets.push(target.to_real(root, id, "bin")?);
            }
        }

        let mut dependencies = Vec::new();
        if let Some(deps) = &self.dependencies {
            for (name, dep) in deps.iter() {
                dependencies.push(dep.to_real(name, root));
            }
        }

        Ok(Package::new(PackageInner{
            id,
            targets,
            dependencies,
            warnings,
            steps: vec![],
        }))
    }
}

impl TomlTarget {
    fn to_real(&self, root: &Path, package: PackageId, target_kind: &str) -> IResult<Target> {
        let kind = if target_kind == "bin" {
            TargetKind::Bin
        } else if self.runtime.is_some() || self.shared.unwrap_or(false) {
            TargetKind::Shared
        } else {
            TargetKind::Static
        };

        let options = if let Some(options) = self.options.as_ref() {
            options.to_real(kind)
        } else {
            Options::default()
        };

        let rpath = if let Some(StringOrBool::String(v)) = &self.runtime {
            Some(PathBuf::from(v))
        } else if let Some(StringOrBool::Bool(true)) = &self.runtime {
            Some(PathBuf::from("."))
        } else {
            None
        };

        let export_header = self.export_header.as_ref()
            .map(|v| abs_path(v, root));

        let mut sources = Vec::new();
        let mut includes = Vec::new();
        let mut defines = Vec::new();
        let mut depends = Vec::new();
        for v in self.sources.as_ref().unwrap_or(&Vec::new()) {
            sources.push(abs_path(v, root));
        }
        for v in self.include_public.as_ref().unwrap_or(&Vec::new()) {
            includes.push(PublicPrivate::public(abs_path(v, root)));
        }
        for v in self.include_private.as_ref().unwrap_or(&Vec::new()) {
            includes.push(PublicPrivate::private(abs_path(v, root)));
        }
        for v in self.define_public.as_ref().unwrap_or(&BTreeSet::new()) {
            defines.push(PublicPrivate::public((v.clone(), None)));
        }
        for v in self.define_private.as_ref().unwrap_or(&BTreeSet::new()) {
            defines.push(PublicPrivate::private((v.clone(), None)));
        }
        for v in self.depends_public.as_ref().unwrap_or(&Vec::new()) {
            depends.push(PublicPrivate::public(v.to_real(package)));
        }
        for v in self.depends_private.as_ref().unwrap_or(&Vec::new()) {
            depends.push(PublicPrivate::private(v.to_real(package)));
        }

        if kind == TargetKind::Shared {
            let name = self.name.to_uppercase();
            defines.push(PublicPrivate::public((format!("{CCARGO_SHARED}{name}"), None)));
            defines.push(PublicPrivate::private((format!("{CCARGO_EXPORT}{name}"), None)));
        }

        Ok(Target::new(TargetInner{
            name: self.name,
            package: package.into(),
            kind,
            options,
            sources,
            defines,
            includes,
            depends,
            rpath,
            export_header,
        }))
    }
}

impl TomlOptions {
    fn to_real(&self, kind: TargetKind) -> Options {
        // TODO: Toml options and inherit from project
        let mut options = Options::default();
        if kind == TargetKind::Shared {
            options.crt = Crt::Shared;
        }
        options
    }
}

impl TomlDependency {
    fn to_real(&self, name: &str, root: &Path) -> Dependency {
        let name: InternedString = name.into();
        match self {
            Self::Simple(..) => unimplemented!("Only path dependencies for now"),
            Self::Detailed(dep) => {
                let path = dep.path.as_deref().expect("Only path dependencies for now");
                Dependency { name, source_id: SourceId::new(abs_path(path, root)) }
            }
        }
    }
}

impl TomlTargetDependency {
    fn to_real(&self, package: PackageId) -> TargetName {
        match self {
            Self::External(v) => v.clone(),
            Self::Local(target) => TargetName::new(package.name(), *target),
        }
    }
}

fn abs_path<P: AsRef<Path>>(path: P, cwd: &Path) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        paths::normalize(cwd.join(path))
    }
}
