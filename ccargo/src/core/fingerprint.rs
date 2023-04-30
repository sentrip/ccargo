use crate::cc::{dep_info, Object};
use crate::core::{Unit, Context, Target, TargetName, Step};
use crate::utils::{IResult, BinaryReader, BinaryWriter, paths, cached_mtime, to_hex, hash_u64};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Arc};
use filetime::FileTime;
use anyhow::bail;


/// Update information collected by `Fingerprint`
#[derive(Debug, Clone, Default)]
pub struct FingerprintState {
    pub files: HashSet<PathBuf>,
    pub link: bool,
}

impl FingerprintState {
    pub fn is_fresh(&self) -> bool {
        self.files.is_empty() && !self.link
    }
}

/// A fingerprint can be considered to be a "short string" representing the
/// state of a world for a target.
#[derive(Default)]
pub struct Fingerprint {
    // Hash of the compiler used (path, mtime)
    compiler_hash: u64,
    // Hash of the `Target` struct
    target_hash: u64,
    // Hash of the `Profile` struct
    profile_hash: u64,
    /// Description of whether the filesystem status for this unit is up to date
    /// or should be considered stale.
    fs_status: FsStatus,
    /// Fingerprints of dependencies.
    deps: Vec<DepFingerprint>,
    /// Information about the inputs that affect this Unit.
    local: Vec<LocalFingerprint>,
    /// Files, relative to `target_root`, that are produced by the step that
    /// this `Fingerprint` represents. This is used to detect when the whole
    /// fingerprint is out of date if this is missing, or if previous
    /// fingerprints output files are regenerated and look newer than this one.
    outputs: Vec<PathBuf>,
    /// Cached hash of the `Fingerprint` struct. Used to improve performance for hashing.
    memoized_hash: Mutex<Option<u64>>,
}


/// Indication of the status on the filesystem for a particular unit.
#[derive(Clone, Default)]
enum FsStatus {
    /// This unit is to be considered stale, even if hash information all
    /// matches. The filesystem inputs have changed (or are missing) and the
    /// unit needs to subsequently be recompiled.
    #[default]
    Stale,

    /// This unit is up-to-date. All outputs and their corresponding mtime are
    /// listed in the payload here for other dependencies to compare against.
    UpToDate { mtimes: HashMap<PathBuf, FileTime> },
}


/// Dependency edge information for fingerprints. This is generated for each
/// dependency and is stored in a `Fingerprint` below.
#[derive(Clone)]
struct DepFingerprint {
    /// The hash of the package id that this dependency points to
    pkg_id: u64,
    /// The target name we're using for this dependency, which if we change we'll need to recompile!
    name: TargetName,
    /// The dependency's fingerprint we recursively point to, containing all the
    /// other hash information we'd otherwise need.
    fingerprint: Arc<Fingerprint>,
}


/// A `LocalFingerprint` represents something that we use to detect direct
/// changes to a `Fingerprint`.
#[derive(Debug, Clone, Hash)]
enum LocalFingerprint {
    /// This is used for crate compilations. The `dep_info` file is a relative
    /// path anchored at `target_root(...)` to the dep-info file that Cargo
    /// generates (which is a custom serialization after parsing rustc's own
    /// `dep-info` output).
    ///
    /// The `dep_info` file, when present, also lists a number of other files
    /// for us to look at. If any of those files are newer than this file then
    /// we need to recompile.
    CheckDepInfo { dep_info: PathBuf, check_all: bool },

    /// This represents a nonempty set of `rerun-if-changed` annotations printed
    /// out by a build script. The `output` file is a relative file anchored at
    /// `target_root(...)` which is the actual output of the build script. That
    /// output has already been parsed and the paths printed out via
    /// `rerun-if-changed` are listed in `paths`. The `paths` field is relative
    /// to `pkg.root()`
    ///
    /// This is considered up-to-date if all of the `paths` are older than
    /// `output`, otherwise we need to recompile.
    RerunIfChanged {
        output: PathBuf,
        paths: Vec<PathBuf>,
    },
}


/// Calculate a fingerprint
pub fn prepare(
    cx: &Context,
    unit: &Unit,
    fingerprint_path: &Path,
) -> IResult<(Arc<Fingerprint>, FingerprintState)> {
    let mut state = FingerprintState::default();    
    let fingerprint = calculate(cx, unit, &mut state)?;
    
    let r = compare_old_fingerprint(&fingerprint, fingerprint_path);
    if r.is_ok() {
        println!("Target `{}` - fresh", unit.full_name());
        return Ok((fingerprint, FingerprintState::default()));
    } else {
        println!("{}", r.err().unwrap());
    }
    println!("Target `{}` - dirty", unit.full_name());
    
    if fingerprint_path.exists() {
        paths::write(fingerprint_path, b"")?;
    }

    Ok((fingerprint, state))
}


/// Write a fingerprint to disk
pub fn write_to_disk(
    fingerprint: &Arc<Fingerprint>,
    fingerprint_path: &Path,
) -> IResult<()> {    
    paths::write(
        fingerprint_path, 
        to_hex(fingerprint.hash_u64())
    )?;
    
    let mut w = BinaryWriter::default();
    fingerprint.serialize(&mut w);
    paths::write(
        fingerprint_path.with_extension("bin"), 
        w.into_inner()
    )?;

    Ok(())
}


/// Calculates the fingerprint for a `unit`.
fn calculate(
    cx: &Context, 
    unit: &Unit,
    state: &mut FingerprintState,
) -> IResult<Arc<Fingerprint>> {
    if let Some(s) = cx.fingerprints.lock().unwrap().get(unit) {
        return Ok(s.clone());
    }

    let target_root = cx.layout.target();
    let pkg_root = unit.package().root();
    
    let (sources, mut fingerprint) = match unit {
        Unit::Target(target) => {
            calculate_target(target, cx, pkg_root, &target_root, state)?
        }
        Unit::Step(step) => {
            calculate_step(step, cx, pkg_root, &target_root, state)?
        }
    };

    fingerprint.check_filesystem(
        pkg_root,
        &target_root,
        sources,
        state,
    )?;
    
    let fingerprint = Arc::new(fingerprint);
    cx.fingerprints.lock().unwrap().insert(unit.clone(), fingerprint.clone());
    Ok(fingerprint)
}


/// Calculates the fingerprint for a `Target` `unit`.
fn calculate_target<'a>(
    target: &'a Target,
    cx: &Context, 
    pkg_root: &Path,
    target_root: &Path,
    state: &mut FingerprintState,
) -> IResult<(&'a [PathBuf], Fingerprint)> {
    let deps = {
        let mut deps = Vec::new();
        let target_deps = &cx.target_deps[target];
        for dep_path in target_deps.libs.iter() {
            if let Some(dep_unit) = cx.units.with_output(dep_path) {
                deps.push(DepFingerprint::new(cx, dep_unit, state)?);
            }
        }
        deps.sort_by(|a, b| a.pkg_id.cmp(&b.pkg_id));
        deps
    };

    let local = {
        let dep_info = target.dep_info_path(cx.layout);
        let dep_info = dep_info.strip_prefix(&target_root).unwrap().to_path_buf();
        vec![LocalFingerprint::CheckDepInfo{ dep_info, check_all: true }]
    };

    let io = &cx.target_io[target];
    let mut outputs = Vec::new();
    outputs.push(io.output.clone());
    for a in io.artifacts.iter() {
        if !a.is_auxiliary() {
            outputs.push(io.output.with_extension(a.ext()));
        }
    }

    Ok((&target.sources, Fingerprint{
        deps,
        local,
        outputs,
        fs_status: FsStatus::Stale,
        compiler_hash: hash_u64(cx.toolchain),
        target_hash: hash_u64(&target.stable_hash(pkg_root)),
        profile_hash: hash_u64(cx.profile),
        memoized_hash: Mutex::default(),
    }))
}


/// Calculates the fingerprint for a `Step` `unit`.
fn calculate_step<'a>(
    step: &'a Step,
    cx: &Context, 
    pkg_root: &Path,
    target_root: &Path,
    state: &mut FingerprintState,
) -> IResult<(&'a [PathBuf], Fingerprint)> {
    let deps = {
        let mut deps = Vec::new();
        for input in step.inputs.iter() {
            if let Some(dep_step) = cx.units.step_with_output(input) {
                deps.push(DepFingerprint::new(cx, &Unit::Step(dep_step.clone()), state)?);
            }
        }
        for dep_name in step.depends.iter() {
            let dep_unit = cx.units.get(dep_name, &step.package);
            deps.push(DepFingerprint::new(cx, dep_unit, state)?);
        }
        deps.sort_by(|a, b| a.pkg_id.cmp(&b.pkg_id));
        deps
    };

    let local = {
        let dep_info = step.dep_info_path(cx.layout);
        let dep_info = dep_info.strip_prefix(target_root).unwrap().to_path_buf();
        vec![LocalFingerprint::CheckDepInfo{ dep_info, check_all: false }]
    };

    Ok((&step.inputs, Fingerprint{
        deps,
        local,
        outputs: step.outputs.clone(),
        fs_status: FsStatus::Stale,
        compiler_hash: 0,
        target_hash: hash_u64(&step.stable_hash(pkg_root)),
        profile_hash: hash_u64(cx.profile),
        memoized_hash: Mutex::default(),
    }))
}

impl DepFingerprint {
    fn new(cx: &Context, unit: &Unit, state: &mut FingerprintState) -> IResult<Self> {
        let fingerprint = calculate(cx, unit, state)?;
        let pkg_id = hash_u64(&unit.package().name());
        Ok(Self{pkg_id, fingerprint, name: unit.full_name()})
    }    
}

impl Fingerprint {
    /// Add `ReRunIfChanged` local fingerprint item
    pub(crate) fn add_rerun_if_changed(&mut self, output: PathBuf, paths: Vec<PathBuf>) {
        use LocalFingerprint::*;
        if let Some(item) = self.local.iter_mut().find(|x| matches!(x, RerunIfChanged { .. })) {
            *item = RerunIfChanged { output, paths };
        } else {
            self.local.push(RerunIfChanged { output, paths })
        }
    }

    /// Dynamically inspect the local filesystem to update the `fs_status` field
    /// of this `Fingerprint`.
    fn check_filesystem(
        &mut self, 
        pkg_root: &Path,
        target_root: &Path,
        sources: &[PathBuf],
        state: &mut FingerprintState,
    ) -> IResult<()> {
        fn all_dirty(state: &mut FingerprintState, sources: &[PathBuf]) -> IResult<()> {
            state.files.extend(sources.iter().cloned());
            state.link = true;
            Ok(())
        }

        let mut mtimes = HashMap::new();

        // Get the `mtime` of all outputs. 
        for output in self.outputs.iter() {
            let mtime = match paths::mtime(output) {
                Ok(mtime) => mtime,
                // This path failed to report its `mtime`. It probably doesn't
                // exist, so leave ourselves as stale and bail out.
                Err(..) => return all_dirty(state, sources),
            };
            assert!(mtimes.insert(output.clone(), mtime).is_none());
        }

        // Get maximum `mtime` of all outputs, or bail out if there are no outputs
        let max_mtime = if let Some(mtime) = mtimes.values().max() {
            mtime
        } else {
            self.fs_status = FsStatus::UpToDate { mtimes };
            return Ok(());
        };

        // Get the `mtime` of all dependencies.
        for dep in self.deps.iter() {
            let dep_mtimes = match &dep.fingerprint.fs_status {
                FsStatus::UpToDate { mtimes } => mtimes,
                // If our dependency is stale, so are we, so bail out.
                FsStatus::Stale => {
                    state.link = true;
                    break;
                },
            };
            let dep_mtime = match dep_mtimes.values().max() {
                Some(dep_mtime) => dep_mtime,
                // If our dependencies is up to date and has no filesystem
                // interactions, then we can move on to the next dependency.
                None => continue,
            };
            
            // If the dependency is newer than our own output then it was
            // recompiled previously. We transitively become stale ourselves in
            // that case, so exit out of the loop. We still need to check
            // which files have been updated to skip compilation if possible.
            if dep_mtime >= max_mtime {
                state.link = true;
                break;
            }
        }
        
        // Check `LocalFingerprint` information to see if we have any stale
        // files for this package itself. If we do find something log a helpful
        // message and bail out so we stay stale.
        for local in self.local.iter() {
            if let Some(item) = local.find_stale_item(pkg_root, target_root, &mut state.files)? {
                // TODO: Enable logging and implement some logging for fingerprint
                if false {
                    item.log();
                }
                if let StaleItem::MissingFile(..) = item {
                    return all_dirty(state, sources);
                } else {
                    state.link = true;            
                    return Ok(());
                }
            }
        }
        
        if !state.link {
            // Everything was up to date! Record such.
            self.fs_status = FsStatus::UpToDate { mtimes };
        }

        Ok(())
    }
    
    /// Compares this fingerprint with an old version which was previously
    /// serialized to filesystem.
    ///
    /// The purpose of this is exclusively to produce a diagnostic message
    /// indicating why we're recompiling something. This function always returns
    /// an error, it will never return success.
    fn compare(&self, old: &Fingerprint) -> IResult<()> {
        use LocalFingerprint::*;
        if self.compiler_hash != old.compiler_hash {
            bail!("compiler has changed")
        }
        if self.target_hash != old.target_hash {
            bail!("target configuration has changed")
        }
        if self.profile_hash != old.profile_hash {
            bail!("profile configuration has changed")
        }
        if self.local.len() != old.local.len() {
            bail!("local lens changed")
        }
        if self.deps.len() != old.deps.len() {
            bail!("number of dependencies has changed")
        }
        for (new, old) in self.local.iter().zip(old.local.iter()) {
            match (new, old) {
                (
                    CheckDepInfo { dep_info: adep, .. },
                    CheckDepInfo { dep_info: bdep, .. },
                ) => {
                    if adep != bdep {
                        bail!(
                            "dep info output changed: previously {:?}, now {:?}",
                            bdep,
                            adep
                        )
                    }
                }

                (
                    RerunIfChanged { output: aout, paths: apaths, },
                    RerunIfChanged { output: bout, paths: bpaths, },
                ) => {
                    if aout != bout {
                        bail!(
                            "rerun-if-changed output changed: previously {:?}, now {:?}",
                            bout,
                            aout
                        )
                    }
                    if apaths != bpaths {
                        bail!(
                            "rerun-if-changed output changed: previously {:?}, now {:?}",
                            bpaths,
                            apaths,
                        )
                    }
                }

                (a, b) => bail!(
                    "local fingerprint type has changed ({} => {})",
                    b.kind(),
                    a.kind()
                ),
            }
        }

        for (a, b) in self.deps.iter().zip(old.deps.iter()) {
            if a.name != b.name {
                let e = anyhow::format_err!("`{}` != `{}`", a.name, b.name)
                    .context("unit dependency name changed");
                return Err(e);
            }

            if a.fingerprint.hash_u64() != b.fingerprint.hash_u64() {
                let e = anyhow::format_err!(
                    "new ({}/{:x}) != old ({}/{:x})",
                    a.name,
                    a.fingerprint.hash_u64(),
                    b.name,
                    b.fingerprint.hash_u64()
                )
                .context("unit dependency information changed");
                return Err(e);
            }
        }

        if !self.fs_status.up_to_date() {
            bail!("current filesystem status shows we're outdated");
        }

        bail!("two fingerprint comparison turned up nothing obvious")
    }

    fn hash_u64(&self) -> u64 {
        if let Some(s) = *self.memoized_hash.lock().unwrap() {
            return s;
        }
        let ret = hash_u64(self);
        *self.memoized_hash.lock().unwrap() = Some(ret);
        ret
    }
}

impl LocalFingerprint {
    fn kind(&self) -> &'static str {
        match self {
            LocalFingerprint::CheckDepInfo { .. } => "dep-info",
            LocalFingerprint::RerunIfChanged { .. } => "rerun-if-changed",
        }
    }

    /// Checks dynamically at runtime if this `LocalFingerprint` has a stale
    /// item inside of it.
    fn find_stale_item(
        &self,
        pkg_root: &Path,
        target_root: &Path,
        updated: &mut HashSet<PathBuf>,
    ) -> IResult<Option<StaleItem>> {
        match self {
            // We need to verify that no paths listed in `paths` are newer than
            // the `output` path itself, or the last time the build script ran.
            LocalFingerprint::RerunIfChanged { output, paths } => Ok(find_stale_file(
                &target_root.join(output),
                paths.iter().map(|p| pkg_root.join(p)),
            )),

            // We need to parse `dep_info`, learn about the crate's dependencies.
            //
            // For each file we see if any of them are newer than
            // the `dep_info` file itself whose mtime represents the start of
            // compilation.
            LocalFingerprint::CheckDepInfo { dep_info, check_all } => {
                let dep_info = target_root.join(dep_info);

                let data = if let Ok(r) = paths::read_bytes(&dep_info) {
                    r
                } else {
                    return Ok(Some(StaleItem::MissingFile(dep_info)));
                };

                let info = if let Some(info) = DepInfo::deserialize(&data) {
                    info
                } else {
                    return Ok(Some(StaleItem::MissingFile(dep_info)));
                };

                let paths = info.files.iter()
                    .map(|(t, p)| t.path(p, pkg_root, target_root))
                    .collect::<Vec<_>>();
                
                let dep_info_mtime = match cached_mtime::mtime(&dep_info) {
                    Ok(mtime) => mtime,
                    Err(..) => return Ok(Some(StaleItem::MissingFile(dep_info))),
                };

                let mut items = Vec::new();
                for path in paths.iter() {
                    if let Some(stale) = stale_item(&dep_info, dep_info_mtime, path) {
                        updated.insert(path.clone());
                        if !check_all {
                            return Ok(Some(stale));
                        }
                        items.push(stale);
                    }
                }

                for obj in info.objects {
                    let src = &paths[obj.file as usize];
                    for input in obj.inputs.iter() {
                        let input = &paths[*input as usize];
                        if let Some(stale) = stale_item(&dep_info, dep_info_mtime, input) {
                            updated.insert(src.clone());
                            if !check_all {
                                return Ok(Some(stale));
                            }
                            items.push(stale);
                            break;
                        }
                    }
                }
                
                Ok(if items.is_empty() {
                    None
                } else if items.len() == 1 {
                    Some(items.pop().unwrap())
                } else {
                    Some(StaleItem::List(items))
                })
            }
        }
    }
}

impl Clone for Fingerprint {
    fn clone(&self) -> Self {
        Fingerprint { 
            compiler_hash: self.compiler_hash.clone(),
            target_hash: self.target_hash.clone(),
            profile_hash: self.profile_hash.clone(),
            fs_status: self.fs_status.clone(),
            deps: self.deps.clone(),
            local: self.local.clone(),
            outputs: self.outputs.clone(),
            memoized_hash: Mutex::default(),
        }
    }
}

impl std::hash::Hash for Fingerprint {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (
            self.compiler_hash,
            self.target_hash,
            self.profile_hash,
            &self.local,
        ).hash(state);

        state.write_usize(self.deps.len());

        for dep in self.deps.iter() {
            dep.pkg_id.hash(state);
            dep.name.hash(state);
            state.write_u64(dep.fingerprint.hash_u64());
        }
    }
}

impl FsStatus {
    fn up_to_date(&self) -> bool {
        matches!(self, Self::UpToDate { .. })
    }
}


/// Compares the fingerprint stored on disk to the new fingerprint provided
fn compare_old_fingerprint(
    fingerprint: &Fingerprint, 
    fingerprint_path: &Path,
) -> IResult<()> {
    let old_hash = paths::read_string(fingerprint_path)?;
    let new_hash = to_hex(fingerprint.hash_u64());
    if old_hash == new_hash && fingerprint.fs_status.up_to_date() {
        return Ok(());
    }
    
    let old_bytes = paths::read_bytes(fingerprint_path.with_extension("bin"))?;
    let mut r = BinaryReader(&old_bytes);
    let old_fingerprint = Fingerprint::deserialize(&mut r)
        .ok_or(anyhow::format_err!("Failed to parse fingerprint"))?;
    
    fingerprint.compare(&old_fingerprint)
}


enum StaleItem {
    List(Vec<StaleItem>),
    MissingFile(PathBuf),
    ChangedFile {
        reference: PathBuf,
        reference_mtime: FileTime,
        stale: PathBuf,
        stale_mtime: FileTime,
    },
}

/// Find a stale file in the list of paths comparing to the reference
fn find_stale_file<I>(reference: &Path, paths: I) -> Option<StaleItem>
where
    I: IntoIterator,
    I::Item: AsRef<Path>,
{
    let reference_mtime = match cached_mtime::mtime(reference) {
        Ok(mtime) => mtime,
        Err(..) => return Some(StaleItem::MissingFile(reference.to_path_buf())),
    };

    for path in paths {
        if let Some(item) = stale_item(reference, reference_mtime, path.as_ref()) {
            return Some(item);
        }
    }

    None
}

/// Get stale item
fn stale_item(
    reference: &Path,
    reference_mtime: FileTime,
    path: &Path,
) -> Option<StaleItem> {
    let path_mtime = match cached_mtime::mtime(path) {
        Ok(mtime) => mtime,
        Err(..) => return Some(StaleItem::MissingFile(path.to_path_buf())),
    };

    if path_mtime >= reference_mtime {
        return Some(StaleItem::ChangedFile {
            reference: reference.to_path_buf(),
            reference_mtime,
            stale: path.to_path_buf(),
            stale_mtime: path_mtime,
        });
    }
    
    None
}


/// Parses `.d` files generated by compiler into a CCargo-specific dep-info format
pub fn translate_dep_info<'a, I: IntoIterator<Item=&'a Object>>(
    objects: I,
    pkg_root: &Path,
    target_root: &Path,
    output_path: &Path,
) -> IResult<()> {
    let mut dep = DepInfo::default();
    
    // TODO: Filter out standard library paths when translating dep info

    let mut idx = HashMap::new();
    let mut all_deps = Vec::new();
    for obj in objects {
        if !idx.contains_key(&obj.src) {
            idx.insert(obj.src.clone(), dep.files.len());
            let path = obj.src.strip_prefix(pkg_root).unwrap().to_path_buf();
            dep.files.push((DepInfoPathType::PackageRootRelative, path));
        }

        let paths = dep_info::read_dependency_file(obj.dep())?;
        for path in paths.iter() {
            if idx.contains_key(path) {
                continue;
            }
            let id = dep.files.len();
            idx.insert(path.clone(), id);
            if let Ok(rel) = path.strip_prefix(pkg_root) {
                dep.files.push((DepInfoPathType::PackageRootRelative, rel.to_path_buf()));
            } else {
                let rel = path.strip_prefix(target_root).unwrap_or(path);
                dep.files.push((DepInfoPathType::TargetRootRelative, rel.to_path_buf()));
            }
        }
        all_deps.push((&obj.src, paths));
    }

    for (src, paths) in all_deps.iter() {
        let file = idx[*src] as u32;
        let mut inputs = Vec::with_capacity(paths.len());
        for path in paths {
            inputs.push(idx[path] as u32);
        }
        dep.objects.push(DepObject { file, inputs })
    }
    
    paths::create_dir_all(output_path.parent().unwrap())?;
    paths::write(output_path, dep.serialize())
}

#[derive(Default)]
pub struct DepInfo {
    files: Vec<(DepInfoPathType, PathBuf)>,
    objects: Vec<DepObject>,
}

struct DepObject {
    file: u32,
    inputs: Vec<u32>,
}

enum DepInfoPathType {
    // src/, e.g. src/lib.rs
    PackageRootRelative,
    // target/debug/deps/lib...
    // or an absolute path /.../sysroot/...
    TargetRootRelative,
}

impl DepInfoPathType {
    fn path(&self, path: &Path, pkg_root: &Path, target_root: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let DepInfoPathType::PackageRootRelative = self {
            pkg_root.join(path)
        } else {
            target_root.join(path)
        }
    }

    fn to_u8(&self) -> u8 {
        if let Self::PackageRootRelative = self {
            0
        } else {
            1
        }
    }

    fn from_u8(value: u8) -> Self {
        if value == 0 {
            Self::PackageRootRelative
        } else {
            Self::TargetRootRelative
        }
    }
}

impl DepInfo {
    fn deserialize(bytes: &[u8]) -> Option<Self> {
        let mut s = Self::default();
        let mut r = BinaryReader(bytes);
        let n_files = r.read_u32()?;
        let n_objs = r.read_u32()?;
        for _ in 0..n_files {
            let kind = DepInfoPathType::from_u8(r.read_u8()?);
            let path = r.read_path()?;
            s.files.push((kind, path));
        }
        for _ in 0..n_objs {
            let file = r.read_u32()?;
            let count = r.read_u32()?;
            let mut inputs = Vec::new();
            for _ in 0..count {
                inputs.push(r.read_u32()?);
            }
            s.objects.push(DepObject { file, inputs })
        }
        Some(s)
    }

    fn serialize(&self) -> Vec<u8> {
        let mut w = BinaryWriter::default();
        w.write_u32(self.files.len() as u32);
        w.write_u32(self.objects.len() as u32);
        for (kind, path) in self.files.iter() {
            w.write_u8(kind.to_u8());
            w.write_path(path);
        }
        for obj in self.objects.iter() {
            w.write_u32(obj.file);
            w.write_u32(obj.inputs.len() as u32);
            for inp in obj.inputs.iter() {
                w.write_u32(*inp);
            }
        }
        w.into_inner()
    }
}

impl Fingerprint {
    fn deserialize(r: &mut BinaryReader) -> Option<Self> {
        let mut v = Self::default();
        v.compiler_hash = r.read_u64()?;
        v.target_hash = r.read_u64()?;
        v.profile_hash = r.read_u64()?;
        let n_deps = r.read_u32()?;
        let n_local = r.read_u32()?;
        for _ in 0..n_deps {
            v.deps.push(DepFingerprint::deserialize(r)?);
        }
        for _ in 0..n_local {
            v.local.push(LocalFingerprint::deserialize(r)?);            
        }
        Some(v)
    }

    fn serialize(&self, w: &mut BinaryWriter) {
        w.write_u64(self.compiler_hash);
        w.write_u64(self.target_hash);
        w.write_u64(self.profile_hash);
        w.write_u32(self.deps.len() as u32);
        w.write_u32(self.local.len() as u32);
        for dep in self.deps.iter() {
            dep.serialize(w);
        }
        for local in self.local.iter() {
            local.serialize(w);
        }
    }
}

impl DepFingerprint {
    fn deserialize(r: &mut BinaryReader) -> Option<Self> {
        let pkg_id = r.read_u64()?;
        let name = r.read_bytes()?;
        let name = std::str::from_utf8(name).ok()?.parse().ok()?;
        let fingerprint = Arc::new(Fingerprint::deserialize(r)?);
        Some(DepFingerprint{pkg_id, name, fingerprint})
    }

    fn serialize(&self, w: &mut BinaryWriter) {
        w.write_u64(self.pkg_id);
        w.write_bytes(self.name.to_string());
        self.fingerprint.serialize(w);
    }
}

impl LocalFingerprint {
    fn deserialize(r: &mut BinaryReader) -> Option<Self> {
        let kind = r.read_u8()?;
        Some(match kind {
            0 => {
                let check_all = r.read_u8()? == 1;
                Self::CheckDepInfo { dep_info: r.read_path()?, check_all }
            }
            1 => {
                let output = r.read_path()?;
                let mut paths = Vec::new();
                let n_paths = r.read_u32()?;
                for _ in 0..n_paths {
                    paths.push(r.read_path()?)
                }
                Self::RerunIfChanged { output, paths }
            }
            _ => unreachable!(),
        })
    }

    fn serialize(&self, w: &mut BinaryWriter) {
        match self {
            Self::CheckDepInfo { dep_info, check_all } => {
                w.write_u8(0);
                w.write_u8(if *check_all { 1 } else { 0 });
                w.write_path(dep_info);
            }
            Self::RerunIfChanged { output, paths } => {
                w.write_u8(1);
                w.write_path(output);
                w.write_u32(paths.len() as u32);
                for path in paths.iter() {
                    w.write_path(path);
                }
            }
        }
    }
}

impl StaleItem {
    /// Use the `log` crate to log a hopefully helpful message in diagnosing
    /// what file is considered stale and why. This is intended to be used in
    /// conjunction with `CCARGO_LOG` to determine why CCargo is recompiling.
    fn log(&self) {
        match self {
            StaleItem::List(items) => {
                for item in items.iter() {
                    item.log();
                }
            }
            StaleItem::MissingFile(path) => {
                println!("stale: missing {:?}", path);
            }
            StaleItem::ChangedFile {
                reference,
                reference_mtime,
                stale,
                stale_mtime,
            } => {
                println!("stale: changed {:?}", stale);
                println!("          (vs) {:?}", reference);
                println!("               {:?} < {:?}", reference_mtime, stale_mtime);
            }
        }
    }
}
