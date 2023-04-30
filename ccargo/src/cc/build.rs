// Modules???

use super::*;
use super::cmd::run_stdout;
use super::{dep_info::write_dependency_file, output::{Message, Extra}, cmd::{run, wait_child, verify_status}};
use crate::utils::{MsgQueue, MsgWriter, ColorString, Color, WriteColorExt};
use std::io::{Read, Write};
use std::process::Command;
use std::hash::Hasher;
use std::ffi::{OsStr, OsString};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use termcolor::{StandardStream, ColorChoice};

/// Directory used for tests
pub const TEST_DIR: &str = "GCCTEST_OUT_DIR";


// Generic message writer used by Build
type Writer = MsgWriter<Box<dyn Write>>;


/// Type of binary that a target will output
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum BinType {
    Static,
    Shared,
    #[default]
    Exe,
}

impl BinType {
    pub fn is_shared(self) -> bool { self == Self::Shared }
    pub fn is_static(self) -> bool { self == Self::Static }
    pub fn is_library(self) -> bool { self != Self::Exe }

    // Get output file extension based on target architecture
    pub fn ext(self, target: &str) -> &'static str {
        if target.contains("windows") {
            match self {
                Self::Static => "lib",
                Self::Shared => "dll",
                Self::Exe => "exe",
            }
        } else {
            match self {
                Self::Static => "a",
                Self::Shared => if target.contains("apple") { "dylib" } else { "so" },
                Self::Exe => "",
            }
        }
    }
}


/// How the messages of the tools will be presented
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    // does not modify the output
    Original,
    // prettifies the output
    Pretty,
    // prettifies and colors the output with standard colors
    Colored,
}


/// Represents the output of the entire build process
#[derive(Default)]
pub struct Output {
    // source/object pairs that were compiled
    pub objs: Vec<(Object, bool)>,
    // path of output executable
    pub path: PathBuf,
    // paths of extra artifacts that were generated during compile
    pub extra: Vec<PathBuf>,
    // was the output binary linked in this build step
    pub did_link: bool,
}


/// Represents a source -> object file pair
#[derive(Debug, Clone)]
pub struct Object {
    // Source file path
    pub src: PathBuf,
    // Output object path
    pub dst: PathBuf,
}

impl Object {
    // Include dependency path
    pub fn dep(&self) -> PathBuf {
        self.dst.with_extension("o.d")
    }
    // Stderr output cache path
    pub fn stderr(&self) -> PathBuf {
        self.dst.with_extension("stderr")
    }
}


/// Represents an output artifact of the build process
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Artifact {
    // MSVC - debug information file
    Pdb,
    // MSVC - link information file
    Ilk,
    // MSVC - shared library import lib
    Lib,
    // MSVC - shared library exports
    Exp,
    // Apple - debug information file
    Dsym,
}

impl Artifact {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Pdb => "pdb",
            Self::Ilk => "ilk",
            Self::Lib => "lib",
            Self::Exp => "exp",
            Self::Dsym => "dSYM",
        }
    }

    pub fn is_debug_info(self) -> bool {
        matches!(self, Self::Pdb | Self::Dsym)
    }

    pub fn is_auxiliary(self) -> bool {
        matches!(self, Self::Ilk | Self::Exp)
    }
}



/// Represents a macro expanded source file
#[derive(Debug, Clone)]
pub struct Expanded {
    // Source file path
    pub src: PathBuf,
    // Macro expanded output
    pub out: Vec<u8>,
}


/// All the parts of a build that can be individually skipped
#[derive(Default)]
struct Skip {
    compile: bool,
    link: bool,
    deps: bool,
    // skips caching the output of stderr
    stderr_cache: bool,
    // if output_cache = false, then stderr will be cached every time a file 
    // is compiled, and will be loaded (if there) when the file is skipped
    files: HashSet<PathBuf>,
}


/// Everything you need to build a C/C++/assembly binary from sources
pub struct Build {
    name: String,
    bin_type: BinType,
    toolchain: Toolchain,
    profile: Profile,
    options: Options,

    files: Vec<PathBuf>,
    includes: Vec<PathBuf>,
    libraries: Vec<PathBuf>,
    objects: Vec<PathBuf>,
    env: Vec<(OsString, OsString)>,
    skip: Skip,

    src_dir: PathBuf,
    out_dir: PathBuf,
    obj_dir: Option<PathBuf>,
    output_cache_path: Option<PathBuf>,
    cwd: Option<PathBuf>,
    host: String,
    
    output_mode: OutputMode,
    force_lang: Option<Language>,
    lang: Language,
    syntax_only: bool,
    not_parallel: bool,
    log_compile: bool,

    cc: Option<Tool>,
    cxx: Option<Tool>,

    stdout: MsgQueue<Box<dyn Write>>,
    stderr: MsgQueue<Box<dyn Write>>,
}

impl Build {
    /// Create a new build for the given toolchain
    pub fn new(name: &str, bin_type: BinType, toolchain: Toolchain) -> Self {
        Self{
            bin_type,
            toolchain,
            name: name.to_string(),
            profile: Profile::dev(),
            options: Options::default(),
            files: Vec::new(),
            includes: Vec::new(),
            libraries: Vec::new(),
            objects: Vec::new(),
            env: Vec::new(),
            skip: Skip::default(),
            src_dir: PathBuf::new(),
            out_dir: PathBuf::new(),
            obj_dir: None,
            output_cache_path: None,
            cwd: None,
            host: host_triple().to_string(),
            output_mode: OutputMode::Colored,
            force_lang: None,
            lang: Language::Cxx,
            syntax_only: false,
            not_parallel: false,
            log_compile: false,
            cc: None,
            cxx: None,
            stdout: MsgQueue::new(0, Box::new(StandardStream::stdout(ColorChoice::Always))),
            stderr: MsgQueue::new(0, Box::new(StandardStream::stderr(ColorChoice::Always))),
        }
    }
    
    /// Set the source folder
    pub fn src_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.src_dir = path.as_ref().to_path_buf();
        self
    }

    /// Set the binary output folder
    pub fn out_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.out_dir = path.as_ref().to_path_buf();
        self
    }

    /// Set the object/dependency (.o/.d) output folder
    pub fn obj_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.obj_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the options used for compilation
    pub fn options(&mut self, options: Options) -> &mut Self {
        self.clear_cached_compilers();
        self.options = options;
        self
    }

    /// Set the profile used for compilation
    pub fn profile(&mut self, profile: Profile) -> &mut Self {
        self.clear_cached_compilers();
        self.profile = profile;
        self
    }
    
    /// Add source file to compile
    pub fn file<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.files.push(path.as_ref().into());
        self
    }
    
    /// Add multiple source files to compile
    pub fn files<P: AsRef<Path>, I: IntoIterator<Item=P>>(&mut self, paths: I) -> &mut Self {
        for p in paths {
            self.files.push(p.as_ref().into());
        }
        self
    }
        
    /// Add a directory to the `-I` or include path for headers - absolute path
    pub fn include<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.includes.push(path.as_ref().into());
        self
    }
    
    /// Add multiple directories to the `-I` or include path for headers - absolute path
    pub fn includes<P: AsRef<Path>, I: IntoIterator<Item=P>>(&mut self, paths: I) -> &mut Self {
        for p in paths {
            self.includes.push(p.as_ref().into());
        }
        self
    }

    /// Add a library to link in - absolute path
    pub fn library<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.libraries.push(path.as_ref().into());
        self
    }
    
    /// Add mulitple libraries to link in - absolute path
    pub fn libraries<P: AsRef<Path>, I: IntoIterator<Item=P>>(&mut self, paths: I) -> &mut Self {
        for p in paths {
            self.libraries.push(p.as_ref().into());
        }
        self
    }
    
    /// Add an arbitrary object file to link in - absolute path
    pub fn object<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.objects.push(path.as_ref().into());
        self
    }

    /// Add many arbitrary object files to link in - absolute path
    pub fn objects<P: AsRef<Path>, I: IntoIterator<Item=P>>(&mut self, paths: I) -> &mut Self {
        for p in paths {
            self.objects.push(p.as_ref().into());
        }
        self
    }

    /// Set path used to store output from compiler tools
    pub fn output_cache<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.output_cache_path = Some(path.as_ref().into());
        self
    }

    /// Request colored output from compiler tools
    pub fn output_mode(&mut self, mode: OutputMode) -> &mut Self {
        self.output_mode = mode;
        self
    }

    /// Force C/C++ to be used instead of detecting based on file extension
    pub fn force_lang(&mut self, lang: Language) -> &mut Self {
        self.force_lang = Some(lang);
        self
    }

    /// Skip compilation of a particular file (object is still linked into binary) - absolute path
    pub fn skip_file<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.skip.files.insert(path.as_ref().into());        
        self
    }
    
    /// Skip the compile step of the build
    pub fn skip_compile(&mut self) -> &mut Self {
        self.skip.compile = true;
        self
    }
            
    /// Skip the link step of the build
    pub fn skip_link(&mut self) -> &mut Self {
        self.skip.link = true;
        self
    }
        
    /// Skip the generation of .d dependency files
    pub fn skip_deps(&mut self) -> &mut Self {
        self.skip.deps = true;
        self
    }

    /// Skip the caching of stderr output for tool invocations
    pub fn skip_output_cache(&mut self) -> &mut Self {
        self.skip.stderr_cache = true;
        self
    }

    /// Print log message for each file that is compiled
    pub fn log_compile(&mut self) -> &mut Self {
        self.log_compile = true;
        self
    }

    /// Pipe stdout output into given writable object
    pub fn stdout<W: Write + 'static>(&mut self, stdout: W) -> &mut Self {
        self.stdout = MsgQueue::new(0, Box::new(stdout));
        self
    }

    /// Pipe stderr output into given message queue
    pub fn stderr<W: Write + 'static>(&mut self, stderr: W) -> &mut Self {
        self.stderr = MsgQueue::new(0, Box::new(stderr));
        self
    }

    /// Get the compiler that's in use for this configuration and caches it for future uses
    pub fn get_compiler(&mut self) -> Result<&Tool, Error> {
        self.get_cache_compilers()?;
        Ok(self.tool())
    }

    // Delete build outputs
    pub fn clean(&mut self) {
        self.ensure_cwd();
        let dst = self.output_path();
        // delete output
        drop(std::fs::remove_file(&dst));
        // delete artifacts
        for a in self.artifacts() {
            drop(std::fs::remove_file(&dst.with_extension(a.ext())));
        }
        // delete object dir
        if let Some(dir) = &self.obj_dir {
            drop(std::fs::remove_dir_all(dir));
        }
    }
    
    /// Run the compiler, checking the syntax of the input files
    pub fn check(&mut self) -> Result<(), Error> {
        self.ensure_cwd();
        let dst = self.output_path();
        let objs = self.object_paths()?;
        self.syntax_only = true;
        self.get_cache_compilers()?;
        self.compile_objects(&dst, &objs)?;
        self.syntax_only = false;
        Ok(())
    }

    /// Run the compiler, generating the output binary file(s)
    pub fn compile(&mut self) -> Result<Output, Error> {
        self.ensure_cwd();
        let dst = self.output_path();
        let objs = self.object_paths()?;

        self.get_cache_compilers()?;
        let compiled = if !self.skip.compile {
            self.compile_objects(&dst, &objs)?
        } else {
            self.skip.compile = false;
            Vec::new()
        };
        self.skip.deps = false;

        let stderr_cache = self.output_cache_path.clone()
            .unwrap_or_else(|| dst.with_extension("stderr"));

        let did_link = if !self.skip.link {
            self.assemble(&dst, &objs, stderr_cache)?;
            true
        } else {
            self.stderr.writer().load_cache_from_path(stderr_cache)?;
            self.skip.link = false;
            false
        };

        let extra = self.artifacts()
            .into_iter()
            .map(|a| dst.with_extension(a.ext()))
            .collect();
        
        Ok(Output { 
            extra,
            did_link,
            path: dst, 
            objs: compiled, 
        })
    }

    /// Run the compiler, expanding preprocessor macros and returning the output
    pub fn expand(&mut self) -> Result<Vec<Expanded>, Error> {
        self.ensure_cwd();
        self.get_cache_compilers()?;
        parallel(
            &self.files,
            self.not_parallel,
            |src| { 
                !self.skip.files.contains(src) 
            },
            |src| {
                let out = self.expand_source(src)?;
                Ok(Expanded{out, src: src.clone()})
            }
        )
    }

    pub fn output_name(
        name: &str,
        bin_type: BinType,
        toolchain: &Toolchain,
    ) -> PathBuf {
        // TODO: lib prefix for library names?
        PathBuf::from(name)
            .with_extension(bin_type.ext(toolchain.target()))        
    }
    
    pub fn output_artifacts(
        bin_type: BinType,
        lang: Language,
        toolchain: &Toolchain,
        profile: &Profile,
    ) -> Vec<Artifact> {
        let mut a = Vec::new();
        if toolchain.tools_for(lang).unwrap().cc.family().is_msvc() {
            if bin_type.is_shared() {
                a.push(Artifact::Lib);
                a.push(Artifact::Exp);
            }
            if profile.debug {
                a.push(Artifact::Pdb);
            }
            if profile.is_incremental() {
                a.push(Artifact::Ilk);
            }
        } else if toolchain.target().contains("apple") {
            if profile.debug {
                a.push(Artifact::Dsym);
            }
        }
        a
    }

}

impl Build {
    #[doc(hidden)]
    pub fn __set_env<A: AsRef<OsStr>, B: AsRef<OsStr>>(&mut self, a: A, b: B) -> &mut Self {
        self.env.push((a.as_ref().to_owned(), b.as_ref().to_owned()));
        self
    }

    fn target(&self) -> &str {
        self.toolchain.target()
    }

    fn tool(&self) -> &Tool {
        if self.lang.is_c() { 
            self.cc.as_ref() 
        } else { 
            self.cxx.as_ref()
        }.unwrap()
    }

    fn cwd(&self) -> &Path {        
        self.cwd.as_deref().expect("Forgot to call `Build::ensure_cwd` before calling `Build::cwd`")
    }

    fn rel<'a>(&'a self, path: &'a Path) -> &'a Path {
        // Passing absolute paths to compiler tools is undesirable due to the need for escaping
        // certain paths (e.g. C://... on windows).
        // Also, more often than not the `source` and `build` directories of a compilation are
        // contained within the same folder (i.e. the project folder), and compilation is executed
        // with the project root as the current directory. Therefore we can attempt to remove 
        // the current directory from any of the paths passed to the compiler.
        // This has the added benefit of making the tool invocations more readable.
        path
            .strip_prefix(self.cwd())
            .unwrap_or(path)
    }

    fn output_path(&self) -> PathBuf {
        let path = self.out_dir
            .join(Self::output_name(&self.name, self.bin_type, &self.toolchain));
        self.rel(&path).to_path_buf()
    }

    fn artifacts(&self) -> Vec<Artifact> {
        Self::output_artifacts(self.bin_type, self.lang, &self.toolchain, &self.profile)
    }

    fn object_paths(&self) -> Result<Vec<Object>, Error> {
        let mut objs = Vec::new();
        let obj_dir = if let Some(dir) = &self.obj_dir {
            dir.clone()
        } else {
            self.out_dir.join(format!("{}.dir", self.name))
        };
        let obj_dir = self.rel(&obj_dir);
        let src_dir = self.rel(&self.src_dir);
        // We want to run tests with files that do not exist so we check here
        let is_test = self.env.iter()
            .map(|v| &v.0)
            .any(|v| v == TEST_DIR);

        for file in self.files.iter() {
            let hashed_name: String;
            // If the file has a parent, prefix the `filename` with
            // a hash of the parent to ensure uniqueness
            let name: &Path = if let Some(parent) = file.parent() {
                let parent = parent
                    .to_str()
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidArgument,
                        &format!(
                            "Failed to convert path to string `{}`", 
                            parent.display()
                        )
                    ))?;

                let fname = file
                    .file_name()
                    .and_then(|v| v.to_str())
                    .ok_or_else(|| Error::new(
                        ErrorKind::InvalidArgument,
                        &format!(
                            "Failed to convert path to string `{}`", 
                            file.display()
                        )
                    ))?;

                let mut h = std::collections::hash_map::DefaultHasher::new();
                h.write(parent.as_bytes());
                hashed_name = format!("{:016x}_{}", h.finish(), fname);
                hashed_name.as_ref()
            } else {
                file.as_ref()
            };

            // Ensure the object file's parent directory exists
            let dst = obj_dir.join(name).with_extension("o");
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Ensure the source file exists
            let src = src_dir.join(file.to_path_buf());
            if !is_test && !src.exists() {
                // Return explicit error for missing files rather than letting the
                // tool executables produce an error as it is faster and more reliable,
                // not to mention much easier to read the cause of the error.
                return Err(Error::new(
                    ErrorKind::InvalidArgument,
                    &format!(
                        "Target `{}` cannot find input source file `{}`", 
                        self.name,
                        src.display()
                    )
                ));
            }
            objs.push(Object{src, dst});
        }

        Ok(objs)
    }

    fn static_crt(&self) -> bool {
        match self.options.crt {
            Crt::Static => true,
            Crt::Shared => false,
            Crt::Default => {
                let shared_ext = Some(OsStr::new(BinType::Shared.ext(self.target())));
                self.libraries.iter()
                    .find(|v| v.extension() == shared_ext)
                    .is_none()
            }
        }
    }

    fn ensure_cwd(&mut self) {
        if self.cwd.is_none() {            
            self.cwd = Some(std::env::current_dir().unwrap_or(PathBuf::new()));
        }
    }

    fn clear_cached_compilers(&mut self) {
        self.cc = None;
        self.cxx = None;
    }

    fn get_cache_compilers(&mut self) -> Result<(), Error> {
        let (detected, path) = if let Some(path) = self.files.iter()
            .find(|v| Language::detect(&v).is_cxx()) 
        {
            (Language::Cxx, Some(path))
        } else {
            (Language::C, None)
        };
        let desired = self.force_lang
            .unwrap_or(detected);
        
        if desired.is_c() && detected.is_cxx() {
            return Err(Error::invalid_arg(format!(
                "Requested C compiler but detected C++ source file `{}`", 
                path.unwrap().display()
            )));
        }

        if !self.toolchain.supports(desired) {
            return Err(Error::invalid_arg(format!("Toolchain does not support {desired}")));
        }

        if self.cc.is_none() && desired.is_c() {
            self.cc = Some(self.get_base_compiler(Language::C)?);
        }

        if self.cxx.is_none() && desired.is_cxx() {
            self.cxx = Some(self.get_base_compiler(Language::Cxx)?);
        }

        self.lang = desired;
        self.stdout.resize(1 + self.files.len());
        self.stderr.resize(1 + self.files.len());
        Ok(())
    }
   
    fn get_base_compiler(&self, lang: Language) -> Result<Tool, Error> {
        let mut tool = self.toolchain.tools_for(lang).unwrap().cc.clone();

        // We add defines and includes later since we might need to 
        // compile with MSVC's assembler instead of the compiler

        self.add_default_compile_flags(&mut tool, lang);
        
        self.add_warnings(&mut tool);

        tool.add_target_flags(self.target())?;
        
        // TODO: Check for flag support?
        for flag in self.options.cc_flags.iter() {
            tool.arg(flag);
        }

        self.verify_args(&tool)?;

        Ok(tool)
    }
    
    fn compile_objects(&self, dst: &Path, objs: &[Object]) -> Result<Vec<(Object, bool)>, Error> {
        if !self.syntax_only {
            // Validate object directory
            if let Some(dir) = &self.obj_dir {
                // In order to not confuse unix executables and directories,
                // the target name must not be the same as the object directory
                if dir == &self.out_dir {
                    let dir_name = dir.file_name().and_then(|v| v.to_str()).unwrap();
                    if dir_name == &self.name {
                        return Err(Error::new(
                            ErrorKind::InvalidArgument, 
                            &format!("Object directory must be different to target name `{}`", dir_name)
                        ));
                    }
                }
            }
        }

        let results = parallel(
            objs,
            self.not_parallel,
            |_| true,
            |obj| {
                if !self.skip.files.contains(&obj.src) {
                    self.compile_object(dst, obj, self.stdout.writer(), self.stderr.writer())?;
                    Ok((obj.clone(), true))
                } else {
                    if !self.skip.stderr_cache {
                        self.stderr.writer().load_cache_from_path(obj.stderr())?;
                    }
                    Ok((obj.clone(), false))
                }
            }
        )?;
        
        Ok(results)
    }
    
    fn compile_object(&self, dst: &Path, obj: &Object, stdout: Writer, mut stderr: Writer) -> Result<(), Error> {
        if !self.skip.stderr_cache {
            stderr.set_cache_path(obj.stderr());
        }

        let target = self.target();
        let asm_ext = AsmFileExt::from_path(&obj.src);
        let is_asm = asm_ext.is_some();
        let is_arm = target.contains("aarch64") || target.contains("arm");
        let msvc = target.contains("msvc");

        let tool = self.tool();
        let (mut cmd, name) = if msvc && asm_ext == Some(AsmFileExt::DotAsm) {
            if self.syntax_only {
                return Err(Error::invalid_arg(format!(
                    "Cannot check the syntax of assembly files: `{}`", 
                    obj.src.display()
                )));
            }
            self.msvc_macro_assembler()
        } else {
            (tool.to_command(), tool.name().to_string())
        };
        
        self.add_defines(&mut cmd);
        self.add_includes(&mut cmd);
        
        if is_asm {
            cmd.args(&self.options.asm_flags);
        }
        
        if self.syntax_only {
            if tool.family().is_msvc() {
                // syntax only
                cmd.arg("-Zs");                
                // generate debug information for object file inside of object file
                if self.profile.debug { cmd.arg("-Z7"); }
            } else {
                cmd.arg("-fsyntax-only");
            };
        } else {
            if tool.family().is_msvc() {                
                // generate debug information for object file in separate PDB
                if self.profile.debug { cmd.arg("-Zi"); }

                // pdb file output location
                let mut s = OsString::from("-Fd");
                s.push(dst.with_extension(Artifact::Pdb.ext()));
                cmd.arg(s);
                // object file output location
                let mut s = OsString::from("-Fo");
                s.push(&obj.dst);
                cmd.arg(s);
            } else {
                if !self.skip.deps {
                    // generate dependency file excluding system headers
                    cmd.arg("-MMD");
                    // dependency file output location
                    cmd.arg("-MF").arg(obj.dep());
                }
                // object file output location
                cmd.arg("-o").arg(&obj.dst);
            }
        }
        // armasm and armasm64 don't require -c option
        if !msvc || !is_asm || !is_arm {
            // compile, but do not link
            cmd.arg("-c");
        }

        cmd.arg(&obj.src);

        // Write message with file name to stderr
        if self.log_compile {
            stderr.push(&{
                let mut msg = ColorString::new();
                for _ in 0..13 { msg.push(' '); }
                let path = self.rel(&obj.src).to_str().unwrap().as_bytes();
                if let OutputMode::Colored = self.output_mode {
                    msg.write_bold(path, Some(Color::Cyan)).unwrap();
                } else {
                    msg.push_bytes(path);
                }
                msg.push('\n');
                msg
            })?;
        }
        
        // TODO: Can we generate dependencies when only checking syntax for MSVC?
        // Msvc does not have its own unix-like dependency generator for C/C++ files
        // The most similar feature is the flag `/scanDependencies`, but that is for C++20 modules, 
        // so we need to manually generate dependencies on windows using the -showIncludes flag.
        let includes = self.run_step(&mut cmd, &name, tool.kind(), tool.family(), stdout, stderr)?;
        if msvc && !self.skip.deps && !self.syntax_only {
            Self::msvc_write_dep_info(tool.path(), &obj.dep(), includes)?;
        }
        Ok(())
    }

    fn expand_source(&self, src: &Path) -> Result<Vec<u8>, Error> {
        let tool = self.tool();
        
        if tool.family().is_msvc() {
            if tool.args().iter().find(|x| x.as_os_str() == "-showIncludes").is_some() {
                return Err(Error::invalid_arg(
                    "Using both args `-showIncludes` and `-E` causes interleaved \
                    stdout output on MSVC. Use `cc::Build::skip_deps()` to prevent this error."
                ))
            }
        }

        let mut cmd = tool.to_command();                
        self.add_defines(&mut cmd);
        self.add_includes(&mut cmd);
        // Preprocess only, do not compile object files
        cmd.arg("-E");
        cmd.arg(src);

        run_stdout(&mut cmd, tool.name())
    }

    fn assemble(&self, dst: &Path, objs: &[Object], stderr_cache: PathBuf) -> Result<(), Error> {
        let (stdout, mut stderr) = (self.stdout.writer(), self.stderr.writer());
        
        if !self.skip.stderr_cache {
            stderr.set_cache_path(stderr_cache);
        }

        // Combine source objects and user-provided objects
        let all_objs: Vec<_> = objs
            .iter()
            .map(|o| o.dst.clone())
            .chain(self.objects.clone())
            .collect();

        // Add objects to the archive in limited-length batches. This helps keep
        // the length of the command line within a reasonable length to avoid
        // blowing system limits on limiting platforms like Windows.
        for (i, chunk) in all_objs.chunks(100).enumerate() {
            // Since we need to link multiple chunks sequentially, we can exit early in the loop
            if self.bin_type.is_static() {
                self.assemble_static(dst, chunk, i == 0, stdout.clone(), stderr.clone())
            } else {
                self.assemble_shared(dst, chunk, stdout.clone(), stderr.clone())
            }?;
        }

        // Non-msvc targets (those using `ar`) need a separate step to add
        // the symbol table to archives since our construction command of
        // `cq` doesn't add it for us.
        if self.bin_type.is_static() && !self.target().contains("msvc") {
            let ar = &self.toolchain.tools_for(self.lang).unwrap().ar;
            let mut cmd = ar.to_command();            
            run(cmd.arg("s").arg(dst), ar.name())?;
        }

        Ok(())
    }

    fn assemble_shared(&self, dst: &Path, objs: &[PathBuf], stdout: Writer, stderr: Writer) -> Result<(), Error> {
        let mut tool = self.toolchain.tools_for(self.lang).unwrap().ld.clone();
    
        let msvc = tool.family().is_msvc();

        self.add_default_link_flags(&mut tool);
        
        for flag in self.options.ld_flags.iter() {
            tool.arg(flag);
        }
        
        let mut cmd = tool.to_command();        

        if !msvc {
            cmd.args(objs);
        }

        self.add_link_outputs(&mut cmd, dst, msvc);

        self.add_libraries(&mut cmd, tool.family());

        if msvc {
            cmd.args(objs);
        }

        self.run_step(
            &mut cmd, 
            tool.name(), 
            tool.kind(), 
            tool.family(), 
            stdout,
            stderr
        )?;
        Ok(())
    }
    
    fn assemble_static(&self, dst: &Path, objs: &[PathBuf], first: bool, stdout: Writer, stderr: Writer) -> Result<(), Error> {
        let ar = &self.toolchain.tools_for(self.lang).unwrap().ar;
        
        let mut cmd = ar.to_command();                

        if ar.family().is_msvc() {
            // suppress microsoft logo
            cmd.arg("-nologo");
            // architecture
            cmd.arg(self.get_msvc_arch_flag());
            // output file
            let mut out = OsString::from("-OUT:");
            out.push(dst);
            cmd.arg(out);
            cmd.args(&self.options.ar_flags);
            // If we are linking in multiple steps, add the library name
            // as an argument to let lib.exe know we are appending the objs.
            if !first { cmd.arg(dst); }
        } else {
            cmd.args(&self.options.ar_flags);
            // c - recreate if exists
            // q - quick create - append to end
            cmd.arg("cq");
            cmd.arg(dst);
        }

        cmd.args(objs);

        self.add_libraries(&mut cmd, ar.family());

        self.run_step(
            &mut cmd, 
            ar.name(), 
            ar.kind(), 
            ar.family(), 
            stdout,
            stderr
        )?;
        Ok(())
    }
    
    fn add_defines(&self, cmd: &mut Command) {
        for v in self.options.defines.iter() {
            cmd.arg(format!("-D{}", v));
        }
        // Non-debug builds
        if !self.profile.debug {
            cmd.arg("-DNDEBUG");
        }
        // TODO: VERSION defines for target
        // TODO: Move STATIC/EXPORTS defines to a place where 
        // it can be propagated between target dependencies
        if self.bin_type.is_library() {
            if self.bin_type.is_static() {
                cmd.arg(format!("-D{}_STATIC", self.name.to_uppercase()));
            } else {
                cmd.arg(format!("-D{}_EXPORTS", self.name.to_uppercase()));
            }
        }
    }

    fn add_includes(&self, cmd: &mut Command) {
        for v in self.includes.iter() {
            cmd.arg("-I").arg(v);
        }
    }

    fn add_libraries(&self, cmd: &mut Command, family: ToolFamily) {
        let target = self.target();
        for v in self.libraries.iter() {
            // msvc and windows-clang only accepts `.lib` files for linking
            if family.is_msvc() || (family.is_clang() && target.contains("windows")) {
                cmd.arg(v.with_extension(Artifact::Lib.ext()));
            } else {
                cmd.arg(v);
            }
        }
        if !self.bin_type.is_static() && family.is_msvc() {
            // msvc system libraries
            cmd.args([
                "kernel32.lib",
                "user32.lib",
                "gdi32.lib",
                "winspool.lib",
                "shell32.lib",
                "ole32.lib",
                "oleaut32.lib",
                "uuid.lib",
                "comdlg32.lib",
                "advapi32.lib",
            ]);
        }
    }

    fn add_link_outputs(&self, cmd: &mut Command, dst: &Path, msvc: bool) {
        if msvc {
            let mut arg = OsString::from("-OUT:");
            arg.push(dst);
            cmd.arg(arg);
        } else {
            cmd.arg("-o").arg(dst);
        }

        for a in self.artifacts() {
            let mut arg = OsString::from(match a {
                Artifact::Pdb => "-PDB:",
                Artifact::Ilk => "-ILK:",
                Artifact::Lib => "-IMPLIB:",
                Artifact::Exp | Artifact::Dsym => continue,
            });
            arg.push(dst.with_extension(a.ext()));
            cmd.arg(arg);
        }
    }

    fn add_warnings(&self, tool: &mut Tool) {
        if tool.family().is_msvc() {
            tool.arg(match self.options.warnings.level {
                WarningLevel::None => "-W0",
                WarningLevel::Default => "-W3",
                WarningLevel::Extra => "-W4",
                WarningLevel::All => "-Wall",
            });
            if self.options.warnings.errors { 
                tool.arg("-WX"); 
            }
        } else {
            const DEFAULT_ERRORS: &[&'static str] = &[
                "-Wall",
            ];
            const EXTRA_ERRORS: &[&'static str] = &[
                "-Wextra",
                "-Wpedantic"
            ];
            const ALL_ERRORS: &[&'static str] = &[
                // TODO: Comprehensive list of warnings on gcc/clang
                "-Wconversion"
            ];
            
            let groups: &[&[&str]] = match self.options.warnings.level {
                WarningLevel::None => &[],
                WarningLevel::Default => &[DEFAULT_ERRORS],
                WarningLevel::Extra => &[DEFAULT_ERRORS, EXTRA_ERRORS],
                WarningLevel::All => &[DEFAULT_ERRORS, EXTRA_ERRORS, ALL_ERRORS],
            };
            for group in groups {
                for flag in group.iter() {
                    tool.arg(flag);
                }
            }
        }
        for flag in self.options.warnings.extra.iter() {
            tool.arg(flag);
        }
    }
    
    fn add_c_cxx_standard_flags(&self, tool: &mut Tool, lang: Language) {
        let num_c = match self.options.std.c {
            StdC::C89 => "89",
            StdC::C99 => "99",
            StdC::C11 => "11",
            StdC::C17 => "17",
            StdC::C20 => "2x",
        };

        let num_cpp = match self.options.std.cxx {
            StdCxx::Cxx98 => "98",
            StdCxx::Cxx11 => "11",
            StdCxx::Cxx14 => "14",
            StdCxx::Cxx17 => "17",
            StdCxx::Cxx20 => "20",
        };

        let (num, prefix) = if lang.is_c() {
            (num_c, if self.options.std.gnu { "gnu" } else { "c" })
        } else {
            (num_cpp, if self.options.std.gnu { "gnu++" } else { "c++" })
        };

        let (sep, supported) = if tool.family().is_msvc() {
            (':', if lang.is_c() {
                matches!(self.options.std.c, StdC::C11 | StdC::C17)
            } else {
                matches!(self.options.std.cxx, StdCxx::Cxx11 | StdCxx::Cxx17)
            })
        } else {
            ('=', true)
        };
        
        if supported {
            tool.arg(format!("-std{}{}{}", sep, prefix, num));
        }
    }

    fn add_cxx_stdlib_flags(&self, tool: &mut Tool) {
        let cxx = if let Some(lib) = &self.options.std.cxx_stdlib {
            lib.as_str()
        } else {
            let target = self.target();
            if target.contains("apple") {
                "c++"
            } else if target.contains("freebsd") {
                "c++"
            } else if target.contains("openbsd") {
                "c++"
            } else if target.contains("android") {
                "c++_shared"
            } else {
                "stdc++"
            }
        };
        tool.arg(format!("-stdlib=lib{cxx}"));
    }

    fn add_debug_flags(&self, tool: &mut Tool) {
        if tool.family().is_msvc() {
            // -Zi/-Z7 is handled in `Build::compile_object`
            // fast runtime error checks
            tool.arg("-RTC1");
            // extra security checks
            // tool.arg("-sdl");
        } else {
            if let Some(dwarf_version) = self.get_dwarf_version() {
                // DWARF debug information on supported targets
                tool.arg(format!("-gdwarf-{dwarf_version}"));
            } else {
                tool.arg("-g");
            }
        }
    }

    fn add_opt_level_flags(&self, tool: &mut Tool) {
        if tool.family().is_msvc() {
            tool.arg(match self.profile.opt_level {
                OptLevel::O0 => "-Od",
                // -O3 is a valid value for gcc and clang compilers, but not msvc. Cap to /O2.
                OptLevel::O2 | OptLevel::O3 => "-O2",
                // Msvc uses /O1 to enable all optimizations that minimize code size.
                OptLevel::Os | OptLevel::Oz | OptLevel::O1 => "-O1",
            });
            
            // inline level
            tool.arg(match self.profile.opt_level {
                OptLevel::O0 | OptLevel::Os | OptLevel::Oz => "-Ob0",
                _ => "-Ob2",
            });
        } else {
            // arm-linux-androideabi-gcc 4.8 shipped with Android NDK does not support '-Oz'
            if self.profile.opt_level == OptLevel::Oz && !tool.family().is_clang() {
                tool.arg("-Os");
            } else {
                tool.arg(match self.profile.opt_level {
                    OptLevel::O0 => "-O0",
                    OptLevel::O1 => "-O1",
                    OptLevel::O2 => "-O2",
                    OptLevel::O3 => "-O3",
                    OptLevel::Os => "-Os",
                    OptLevel::Oz => "-Oz",
                });
            }
        }
    }

    fn add_lto_flags(&self, tool: &mut Tool) {
        if tool.family().is_msvc() {
            // whole program optimization
            tool.arg("-GL");
            // link time code generation
            tool.arg("-LTCG");
        } else {
            if tool.family().is_clang() {
                // lto is not supported in clang on Windows
                if self.target().contains("windows") {
                    return;
                }
                if let Lto::Thin = self.profile.lto {
                    tool.arg("-flto=thin");
                } else {
                    tool.arg("-flto");
                }
            } else {
                tool.arg("-flto");
            }
        }
    }

    fn add_default_compile_flags(&self, tool: &mut Tool, lang: Language) {
        let target = self.target();

        if tool.family().is_msvc() {
            // suppress microsoft logo
            tool.arg("-nologo");
            // functions are __cdecl unless otherwise specified
            tool.arg("-Gd");
            // buffer security checks
            // tool.arg("-GS"); is default
            // floating point behaviour
            tool.arg("-fp:precise");
            // C macro conformance (mostly __VA_OPT__)
            tool.arg("-Zc:preprocessor");
            // C conformance
            tool.arg("-Zc:inline").arg("-Zc:wchar_t").arg("-Zc:forScope");
            // error reporting
            tool.arg("-external:W3").arg("-diagnostics:column");
            // force C/C++
            tool.arg(if lang.is_c() { "-TC" } else { "-TP" });
            // static/dynamic/debug crt
            tool.arg(match (self.static_crt(), self.profile.debug) {
                (true, true) => "-MTd",
                (false, true) => "-MDd",
                (true, false) => "-MT",
                (false, false) => "-MD",
            });
            // show all includes
            if !self.skip.deps { 
                tool.arg("-showIncludes"); 
            }
            // disable exceptions (kinda)
            if lang.is_cxx() && !self.profile.exceptions { 
                tool.arg("-EHsc"); 
            }
            // windows defines
            tool.arg("-DWIN32");
            tool.arg("-D_WINDOWS");
            // utf8 (multi-byte characters) define
            tool.arg("-D_MBCS");
            // shared lib define
            if self.bin_type.is_shared() { 
                tool.arg("-D_WINDLL").arg("-D_USRDLL"); 
            }

        } else {
            // functions are not exported in shared libraries by default
            tool.arg("-fvisibility=hidden");
            if lang.is_cxx() {
                tool.arg("-fvisibility-inlines-hidden");
            }

            // ensure colored output
            if self.output_mode == OutputMode::Colored {
                if tool.family().is_clang() {
                    tool.arg("-fcolor-diagnostics").arg("-fansi-escape-codes");
                } else {
                    tool.arg("-fdiagnostics-color=always");
                }
            }

            // disable exceptions
            if lang.is_cxx() && !self.profile.exceptions { 
                tool.arg("-fno-exceptions"); 
            }
            
            if tool.family() == ToolFamily::Clang && target.contains("android") {
                // For compatibility with code that doesn't use pre-defined `__ANDROID__` macro.
                // If compiler used via ndk-build or cmake (officially supported build methods)
                // this macros is defined.
                // See https://android.googlesource.com/platform/ndk/+/refs/heads/ndk-release-r21/build/cmake/android.toolchain.cmake#456
                // https://android.googlesource.com/platform/ndk/+/refs/heads/ndk-release-r21/build/core/build-binary.mk#141
                tool.arg("-DANDROID");
            }
            
            if !target.contains("apple-ios") && !target.contains("apple-watchos") {
                // reduce binary size of executable
                tool.arg("-ffunction-sections");
                tool.arg("-fdata-sections");
            }

            if !target.contains("windows") {
                if self.bin_type.is_shared() && self.options.unix.pic {
                    // position independent code
                    tool.arg("-fPIC");
                }
                if self.options.unix.force_frame_pointer {
                    // force generation of instructions to emit stack frame pointer
                    tool.arg("-fno-omit-frame-pointer");
                }
                // PLT only applies if code is compiled with PIC support,
                // and only for ELF targets.
                if target.contains("linux") 
                    && self.options.unix.pic 
                    && !self.options.unix.plt 
                {
                    // do not use PLT (procedure linkage table)
                    tool.arg("-fno-plt");
                }
            }
        }

        self.add_opt_level_flags(tool);

        if self.profile.debug {
            self.add_debug_flags(tool);
        } else if self.profile.is_lto_enabled() { 
            self.add_lto_flags(tool);
        }        

        self.add_c_cxx_standard_flags(tool, lang);

        if lang.is_cxx() && tool.family().is_clang() {
            self.add_cxx_stdlib_flags(tool);
        }
    }

    fn add_default_link_flags(&self, tool: &mut Tool) {
        // TODO: -rdynamic | -Wl,--export-dynamic | .exp files (MSVC) (i.e. exporting symbols from exe -> dll)
        // The only legitimate use case for this behaviour is so that debugging tools can
        // extract prettier stack-traces from the executable being debugged.
        let target = self.target();
        if tool.family().is_msvc() {
            // suppress microsoft logo
            tool.arg("-nologo");
            // architecture
            tool.arg(self.get_msvc_arch_flag());
            // address-space layout randomization
            tool.arg("-DYNAMICBASE");
            // data execution prevention
            tool.arg("-NXCOMPAT");
            // incremental linking
            if self.profile.is_incremental() { 
                tool.arg("-INCREMENTAL"); 
            } else {
                tool.arg("-INCREMENTAL:NO");
            }
            // debug symbols
            if self.profile.debug { 
                tool.arg("-DEBUG"); 
            }
            // shared library flag
            if self.bin_type.is_shared() { 
                tool.arg("-DLL"); 
            }
            // treat linker warnings as errors
            if self.options.warnings.errors { 
                tool.arg("-WX"); 
            }
            // TODO: MSVC entry point -ENTRY
            // TODO: MSVC manifest setup
        } else {
            // TODO: Set the soname flag? -Wl,soname,$OUTPUT
            
            // shared library flag
            if self.bin_type.is_shared() { 
                tool.arg("-shared");
            }
            // link to static c/unix libraries
            else if self.static_crt() {
                tool.arg("-static");
            }
            // treat linker warnings as errors
            if self.options.warnings.errors {
                tool.arg("--fatal-warnings");
            }
            // unix-only flags
            if !target.contains("windows") {
                
                if self.profile.is_lto_enabled() {
                    self.add_lto_flags(tool);
                }

                if self.bin_type.is_shared() && self.options.unix.pic {
                    // position independent code
                    tool.arg("-fPIC");
                }

                if !self.bin_type.is_library() {
                    // relative paths for runtime-loading of libraries - create relocatable library
                    tool.arg(format!("-Wl,-rpath,{}", self.profile.rpath));
                }
            }
        }
    }

    fn get_dwarf_version(&self) -> Option<u32> {
        // Tentatively matches the DWARF version defaults as of rustc 1.62.
        let target = self.target();
        if target.contains("android")
            || target.contains("apple")
            || target.contains("dragonfly")
            || target.contains("freebsd")
            || target.contains("netbsd")
            || target.contains("openbsd")
            || target.contains("windows-gnu")
        {
            Some(2)
        } else if target.contains("linux") {
            Some(4)
        } else {
            None
        }
    }

    fn get_msvc_arch_flag(&self) -> String {
        let target = self.target();
        let arch = if target.contains("x86_64") {
            "x64"
        } else {
            "x86"
        };
        let mut out = String::from("-machine:");
        out.push_str(arch);
        out
    }

    // Msvc requires special tools for compiling `.asm` files
    fn msvc_macro_assembler(&self) -> (Command, String) {
        let target = self.target();
        let tool = if target.contains("x86_64") {
            "ml64.exe"
        } else if target.contains("arm") {
            "armasm.exe"
        } else if target.contains("aarch64") {
            "armasm64.exe"
        } else {
            "ml.exe"
        };
        let mut cmd = cc::windows_registry::find(&target, tool)
            .unwrap_or_else(|| Command::new(tool));
        
        // undocumented, yet working with armasm[64]
        cmd.arg("-nologo");

        if target.contains("aarch64") || target.contains("arm") {
            if self.profile.debug {
                cmd.arg("-g");
            }
        } else {
            if self.profile.debug {
                cmd.arg("-Zi");
            }
        }

        if target.contains("i686") || target.contains("i586") {
            cmd.arg("-safeseh");
        }

        cmd.args(&self.options.cc_flags);

        (cmd, tool.to_string())
    }
    
    // Msvc does not allow you to filter standard header includes with `-showIncludes`
    fn msvc_write_dep_info(tool_path: &Path, out_path: &Path, includes: Vec<PathBuf>) -> std::io::Result<()> {
        // Compiler
        //  -> C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.33.31629\bin\Hostx64\x64\cl.exe
        // Include
        //  -> C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.33.31629\include\stdint.h
        let mut msvc_root = tool_path.to_path_buf();
        for _ in 0..4 {
            if !msvc_root.pop() {
                return Ok(());
            }
        }
        let filtered: Vec<_> = includes.into_iter()
            .filter(|p| !p.starts_with(&msvc_root))
            .collect();
        if !filtered.is_empty() {
            write_dependency_file(out_path, &filtered)?;
        }
        Ok(())
    }

    fn verify_args(&self, tool: &Tool) -> Result<(), Error> {
        let mut seen = HashSet::new();
        for arg in tool.args() {
            let key = self.arg_key(tool, arg.to_str().unwrap());
            if !seen.insert(key) {
                // TODO: warning duplicate arg
            }
        }
        Ok(())
    }

    // Get the unique key used to identify a compiler argument
    fn arg_key<'a>(&self, tool: &Tool, arg: &'a str) -> &'a str {
        if tool.family().is_msvc() {
            if let Some(sep) = arg.find(':').or_else(|| arg.find('=')) {
                // Flags that can be specified multiple times must use the entire
                // arg for uniqueness detection rather than just the arg key
                // Also, the first character is skippped so that duplicates can
                // be detected even if flag prefixes are mixed, e.g. `-Zc` and `/Zc`.
                match &arg[1..sep] {
                    "Zc" => return &arg[1..],
                    key => return key,
                }
            }
        } else {
            // Linker flags take the form `-Wl,-flag,value`
            // Assembler flags take the form `-Wa,-flag,value`
            if arg.starts_with("-Wl") || arg.starts_with("-Wa") {
                if let Some(sep) = arg.rfind(',') {
                    if sep > arg.find(',').unwrap() {
                        return &arg[..sep];
                    }
                }
            } else if let Some(sep) = arg.find('=') {
                return &arg[..sep];
            }
        }
        arg
    }

    fn run_step(
        &self, 
        cmd: &mut Command, 
        name: &str, 
        kind: ToolKind,
        family: ToolFamily,
        stdout: Writer,
        stderr: Writer,
    ) -> Result<Vec<PathBuf>, Error> {
        for &(ref a, ref b) in self.env.iter() {
            cmd.env(a, b);
        }
        let mut child = run(cmd, name)?;
        let includes = if let OutputMode::Original = self.output_mode {
            Self::forward_output(child.stderr.take().unwrap(), stderr);
            Self::forward_output(child.stdout.take().unwrap(), stdout);
            Vec::new()
        } else {
            if family.is_msvc() {
                let includes = self.iter_messages(
                    kind, 
                    family, 
                    child.stdout.take().unwrap(),
                    // TODO: Should we send stdout messages to stderr on MSVC?
                    stderr.clone()
                );
                Self::forward_output(child.stderr.take().unwrap(), stderr);
                includes
            } else {
                self.iter_messages(
                    kind, 
                    family, 
                    child.stderr.take().unwrap(), 
                    stderr
                );
                Self::forward_output(child.stdout.take().unwrap(), stdout);
                Vec::new()
            }
        };
        let status = wait_child(cmd, name, &mut child)?;
        verify_status(cmd, name, status)?;
        Ok(includes)
    }

    fn iter_messages<R: Read>(
        &self,
        kind: ToolKind,
        family: ToolFamily,
        input: R,
        mut output: Writer,
    ) -> Vec<PathBuf> {
        let windows = self.host.contains("windows");
        let colors = self.output_mode == OutputMode::Colored;
        let mut includes = Vec::new();
        for msg in Message::iter(
            input,
            kind,
            family,
            windows,
            colors
        ) {
            match msg {
                Message::Extra(Extra::IncludePath(path)) => {
                    includes.push(path);
                }
                msg => drop(msg.print(&mut output, colors)),
            }
        }
        includes
    }

    fn forward_output<R: Read>(
        mut input: R,
        output: Writer,
    ) {
        let mut out = Vec::new();
        drop(input.read_to_end(&mut out));
        if !out.is_empty() {
            drop(output.push(&out));
        }
    }
}


// Helper for detecting `.asm` file extension
#[derive(Clone, Copy, PartialEq)]
enum AsmFileExt {
    /// `.asm` files. On MSVC targets, we assume these should be passed to MASM
    /// (`ml{,64}.exe`).
    DotAsm,
    /// `.s` or `.S` files, which do not have the special handling on MSVC targets.
    DotS,
}

impl AsmFileExt {
    fn from_path(file: &Path) -> Option<Self> {
        match &*file.extension()?.to_str()?.to_lowercase() {
            "asm" => Some(AsmFileExt::DotAsm),
            "s" => Some(AsmFileExt::DotS),
            _ => None,
        }
    }
}


fn parallel<T, F, E, R>(
    items: &[T],
    not_parallel: bool,
    filter: F,
    exec: E,
) -> Result<Vec<R>, Error>
where 
    T: Clone + Send + Sync,
    F: Fn(&T) -> bool + Sync,
    E: Fn(&T) -> Result<R, Error> + Sync,
    R: Send,

{
    use rayon::prelude::*;

    let mut outputs = Vec::new();

    // If we are only iterating a single item or not in parallel, just iterate directly
    if not_parallel || items.len() == 1 {
        for item in items {
            if filter(item) {
                outputs.push(exec(item)?);
            }
        }
        return Ok(outputs);
    }
    
    // Iterate items in parallel with early exit for any errors
    let error = AtomicBool::new(false);
    let results: Vec<_> = items.par_iter()
        .filter_map(|item| {
            if error.load(SeqCst) || !filter(item) {
                return None;
            }
            Some(match exec(item) {
                Ok(r) => Ok(r),
                Err(e) => {
                    error.store(true, SeqCst);
                    Err(e)
                }
            })
        })
        .collect();
    
    for r in results {
        outputs.push(r?);
    }

    Ok(outputs)
}
