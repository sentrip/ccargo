
use super::{Error, Language};
use super::cmd::{run_stdout, run_stderr};
use super::platform::{host_triple, validate_target};
use std::fmt::{self, Write};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use semver::Version;


// Kind of tool used (compiler, linker, archiver)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Compiler,
    Linker,
    Archiver,
}


// Family of tools used (MSVC, GNU, ...)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolFamily {
    Gnu,
    Clang,
    Msvc,
}

impl ToolFamily {
    pub fn is_msvc(self) -> bool { self == Self::Msvc }
    pub fn is_gnu(self) -> bool { self == Self::Gnu }
    pub fn is_clang(self) -> bool { self == Self::Clang }
}


// Represents a complete set of tools that can be used to create 
// static/shared libraries and executables from C/C++/asm sources
#[derive(Debug, Clone, Hash)]
pub struct Toolchain {
    inner: Arc<Inner>,
}

#[derive(Debug, Hash)]
pub struct Inner {
    target: String,
    c: Option<Tools>,
    cxx: Option<Tools>,
}


// Represents a complete set of tools that can be used to create 
// static/shared libraries and executables from sources
// for a particular target and language
#[derive(Debug, Clone, Hash)]
pub struct Tools {
    // Compiler         (foo.c    ->   foo.o)
    pub cc: Tool,
    // Dynamic linker   (foo.o    ->   foo.so/foo.dll)
    pub ld: Tool,
    // Static archiver  (foo.o    ->   foo.a/foo.lib)
    pub ar: Tool,
}


// Represents an invocable compiler tool that contains
// all required arguments/environment variables to be run
#[derive(Clone)]
pub struct Tool {
    kind: ToolKind,
    family: ToolFamily,
    path: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    env_remove: Vec<OsString>,
}


impl Toolchain {
    // Find the default toolchain for the host target
    pub fn default() -> Result<Self, Error> {
        Self::new_priv(host_triple(), None, None)
    }

    // Find toolchain for the given target optionally specifying C/C++ compiler paths
    pub fn new(
        target: &str,
        cc: impl Into<Option<PathBuf>>, 
        cxx: impl Into<Option<PathBuf>>,
    ) -> Result<Self, Error> {
        Self::new_priv(target, cc.into(), cxx.into())
    }

    // Find toolchain for the host target optionally specifying C/C++ compiler paths
    pub fn new_host(
        cc: impl Into<Option<PathBuf>>, 
        cxx: impl Into<Option<PathBuf>>,
    ) -> Result<Self, Error> {
        Self::new_priv(host_triple(), cc.into(), cxx.into())
    }

    // The target that this toolchain can compile to
    pub fn target(&self) -> &str {
        &self.inner.target
    }
    
    // Whether this toolchain can compile sources for the given language
    pub fn supports(&self, lang: Language) -> bool {
        if lang.is_c() {
            // C is supported by C and C++ compilers
            true
        } else {
            self.inner.cxx.is_some()
        }
    }
    
    // C/C++ tools (whichever is supported, C++ preferred, one is guaranteed)
    pub fn tools(&self) -> &Tools {
        self.tools_for(Language::Cxx)
            .or_else(|| self.tools_for(Language::C))
            .unwrap()
    }

    // Tools for given language if supported (C falls back to C++ if it is not natively supported)
    pub fn tools_for(&self, lang: Language) -> Option<&Tools> {
        if let Language::C = lang {
            // Toolchain must have either C or C++ tools, both of which
            // are valid for C, so we try get C tools but fall back to
            // C++ which is guaranteed to exist, so we use unwrap
            self.inner.c.as_ref().or(self.inner.cxx.as_ref())
        } else {
            // You can only compile C++ with C++ tools, so the toolchain must have them
            self.inner.cxx.as_ref()
        }
    }

    fn new_priv(target: &str, cc_path: Option<PathBuf>, cxx_path: Option<PathBuf>) -> Result<Self, Error> {
        // Validate target
        if let Err(e) = validate_target(target) {
            return Err(Error::invalid_arg(format!("{}", e)))
        }

        let cc_path_copy = cc_path.clone();
        let cxx_path_copy = cc_path.clone();
        let provided_cc = cc_path.is_some();
        let provided_cxx = cxx_path.is_some();

        let c = Tools::new(target, cc_path, Language::C);
        let cxx = Tools::new(target, cxx_path, Language::Cxx);

        // Failed to find both C and C++
        // We need at least a C or C++ compiler
        if c.is_err() && cxx.is_err() {
            let mut extra = String::new();
            if let Err(e) = c { write!(extra, "\n{}", e).unwrap(); }
            if let Err(e) = cxx { write!(extra, "\n{}", e).unwrap(); }
            return Err(Error::tool_not_found(format!(
                "Failed to find any C or C++ compilers for target `{target}`{extra}"
            )));
        }
        
        // User wanted C and failed to find C
        let c = if provided_cc {
            match c {
                Ok(c) => Some(c),
                Err(e) => return Err(Error::tool_not_found(format!(
                    "Tool at path `{}` is not a valid C compiler: {e}",
                    cc_path_copy.unwrap().display()
                )))
            }
        } else {
            c.ok()
        };

        // User wanted C++ and failed to find C++
        let cxx = if provided_cxx {
            match cxx {
                Ok(cxx) => Some(cxx),
                Err(e) => return Err(Error::tool_not_found(format!(
                    "Tool at path `{}` is not a valid C++ compiler: {e}",
                    cxx_path_copy.unwrap().display()
                )))
            }
        } else {
            cxx.ok()
        };
        
        // Check paths exist
        if let Some(tools) = &c {
            if !tools.cc.path().exists() {
                return Err(Error::tool_not_found(format!(
                    "CC path does not exist: `{}`", tools.cc.path().display()
                )));
            }
        }        
        if let Some(tools) = &cxx {
            if !tools.cc.path().exists() {
                return Err(Error::tool_not_found(format!(
                    "CXX path does not exist: `{}`", tools.cc.path().display()
                )));
            }
        }

        Ok(Toolchain{inner: Arc::new( Inner { c, cxx, target: target.to_string() } ) })
    }
}

impl Tools {
    fn new(target: &str, path: Option<PathBuf>, lang: Language) -> Result<Self, Error> {
        let from_path = path.is_some();

        // Try get compilers from paths, or select compiler for target and language
        let cc = path
            .and_then(which)
            .map(|p| Tool::new_compiler(p))
            .or_else(|| Tool::compiler(target, lang))
            .ok_or_else(|| Error::tool_not_found(format!(
                "Failed to find {lang} compiler for target `{target}`"
            )))?;

        // Get linkers for compiler if found
        let ld = cc.linker(target, from_path)
            .ok_or_else(|| Error::tool_not_found(format!(
                "Failed to find {lang} linker for target `{target}`"
            )))?;
        
        // Get archiver for compiler if found
        let ar = Tool::archiver(target, cc.family(), from_path)
            .ok_or_else(|| Error::tool_not_found(format!(
                "Failed to find {lang} static archiver for target `{target}`"
            )))?;
        
        Ok(Tools{cc, ld, ar})
    }
}

impl Tool {
    // Create tool from path and try to detect the family of tool from the path
    pub fn new(kind: ToolKind, path: PathBuf) -> Self {
        let (family, path) = Self::detect_family(path);
        Tool {
            path,
            family,
            kind,
            args: Vec::new(),
            env: Vec::new(),
            env_remove: Vec::new(),
        }
    }
    
    // Create compiler from path
    pub fn new_compiler(path: impl AsRef<Path>) -> Self {
        Self::new(ToolKind::Compiler, path.as_ref().to_path_buf())
    }

    // Create archiver from path
    pub fn new_archiver(path: impl AsRef<Path>) -> Self {
        Self::new(ToolKind::Archiver, path.as_ref().to_path_buf())
    }

    // Tool kind (Compiler, Linker, ...)
    pub fn kind(&self) -> ToolKind {
        self.kind 
    }

    // Tool family (MSVC, Clang, ...)
    pub fn family(&self) -> ToolFamily {
        self.family 
    }

    // Path  to the tool's executable
    pub fn path(&self) -> &Path {
        &self.path 
    }

    // Command line args used by this tool
    pub fn args(&self) -> &[OsString] {
        &self.args 
    }

    // Environment variables used by this tool
    pub fn env(&self) -> &[(OsString, OsString)] {
        &self.env
    }

    // Human-readable name of the tool
    pub fn name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap()
    }

    // Add a command line argument to the execution
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().into());
        self
    }

    // Add required flags for target architecture
    pub fn add_target_flags(&mut self, target: &str) -> Result<(), Error> {
        match self.family {
            ToolFamily::Msvc => self.add_target_flags_msvc(target),
            ToolFamily::Clang => self.add_target_flags_clang(target),
            ToolFamily::Gnu => self.add_target_flags_gcc(target),
        }?;
        if target.contains("apple-ios") || target.contains("apple-watchos") {
            self.add_ios_watchos_flags(target)?;
        }
        if let ToolKind::Compiler = self.kind {
            self.fix_env_for_apple_os(target)?;
        }
        Ok(())
    }

    // Detect the version of this tool
    pub fn detect_version(&self) -> Result<Version, Error> {
        let mut cmd = Command::new(&self.path);
        // Msvc writes version in header printed with every call to cl.exe
        if !self.family.is_msvc() {
            cmd.arg("--version");
        }
        
        // GCC writes version to stdout, rest to stderr
        let data = if self.family.is_gnu() {
            run_stdout(&mut cmd, self.name())
        } else {
            run_stderr(&mut cmd, self.name())
        }?;
        
        // Output should be ascii so we can use unwrap
        parse_version(self.family, std::str::from_utf8(&data).unwrap())
    }
    
    /// Converts this tool into a `Command` that's ready to be run.
    pub fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.path);
        cmd.args(&self.args);
        for &(ref k, ref v) in self.env.iter() {
            cmd.env(k, v);
        }
        for k in self.env_remove.iter() {
            cmd.env_remove(k);
        }
        if let ToolKind::Archiver = self.kind {
            macos_ar_ensure_file_times_are_zero(&mut cmd);
        }
        cmd
    }

    // Try to detect family of a tool from its name, falling back to Gnu.
    fn detect_family(path: PathBuf) -> (ToolFamily, PathBuf) {
        let fname = path.file_name().and_then(|p| p.to_str()).unwrap_or("");
        let family = if fname.ends_with("cl") || fname == "cl.exe" {
            ToolFamily::Msvc
        } else if fname.contains("clang") {
            ToolFamily::Clang
        } else {
            // If user passed cc or c++, try detect underlying tool
            if fname == "cc" || fname == "c++" {
                if let Ok(abs) = std::fs::canonicalize(&path) {
                    return Self::detect_family(abs);
                }
            }
            ToolFamily::Gnu
        };
        (family, path)
    }
    
    // Get the linker for this compiler - used for dynamic linking
    // msvc         -> link.exe
    // other        -> copy of compiler (to massively simplify linker invocation)
    fn linker(&self, target: &str, from_path: bool) -> Option<Tool> {
        if self.kind != ToolKind::Compiler {
            return None;
        }
        if self.family.is_msvc() {
            // We need a way to short circuit choosing a linker 
            // so test code can run the mock executable instead
            // of the real linker, and this seems like an unintrusive
            // way to achieve that
            if from_path {
                Some(Tool::new(ToolKind::Linker, self.path.clone()))
            } else {
                msvc_tool("link.exe", target)
            }
        } else {
            let mut tool = self.clone();
            tool.kind = ToolKind::Linker;
            Some(tool)
        }
    }
    
    // Get the compiler for given target - used for compilation
    // msvc        -> cl.exe
    // windows-gnu -> [gcc/g++, clang/clang++]
    // other       -> cc/c++
    // emscripten
    //      windows
    //          cmd /c (emcc/ecm++).bat
    //      other
    //          emcc/ecm++
    fn compiler(target: &str, language: Language) -> Option<Tool> {
        let host = host_triple();

        let (gnu, traditional, clang) = if language.is_cxx() {
            ("g++", "c++", "clang++")
        } else {
            ("gcc", "cc", "clang")
        };

        // On historical Solaris systems, "cc" may have been Sun Studio, which
        // is not flag-compatible with "gcc".  This history casts a long shadow,
        // and many modern illumos distributions today ship GCC as "gcc" without
        // also making it available as "cc".
        let default = if host.contains("solaris") || host.contains("illumos") {
            gnu
        } else {
            traditional
        };
        
        let clang_or_gnu = if target.contains("llvm") { 
            clang 
        } else { 
            gnu 
        };

        let program =  if host.contains("windows") && target.contains("windows") {
            if target.contains("msvc") {
                return msvc_tool("cl.exe", target);
            } else {
                clang_or_gnu.to_string()
            }
        } else if target.contains("emscripten") {
            let cc = if language.is_cxx() { "em++" } else { "emcc" };
            return Some(emscripten_tool(cc)?);
        } else if target.contains("apple-ios") {
            clang.to_string()
        } else if target.contains("apple-watchos") {
            clang.to_string()
        } else if target.contains("android") {
            autodetect_android_compiler(&target, &host, gnu, clang)
        } else if target.contains("cloudabi") {
            format!("{}-{}", target, traditional)
        } else if target == "wasm32-wasi"
            || target == "wasm32-unknown-wasi"
            || target == "wasm32-unknown-unknown"
        {
            clang.to_string()
        } else if target.contains("vxworks") {
            let cc = if language.is_cxx() { "wr-c++" } else { "wr-cc" };
            cc.to_string()
        } else if target.starts_with("armv7a-kmc-solid_") {
            format!("arm-kmc-eabi-{}", gnu)
        } else if target.starts_with("aarch64-kmc-solid_") {
            format!("aarch64-kmc-elf-{}", gnu)
        } else if host != target {
            match prefix_for_target(&target, language) {
                Some(prefix) => format!("{}-{}", prefix, clang_or_gnu),
                None => return Some(Tool::new_compiler(which(default)?)),
            }
        } else {
            return Some(Tool::new_compiler(which(default)?))
        };
        let mut tool = Tool::new_compiler(which(program)?);
        
        // For android we use the default clang compiler with a `--target=` argument
        if host.contains("windows") && android_clang_compiler_uses_target_arg_internally(&tool.path) {
            android_get_default_clang_compiler_target_flag(&mut tool);
        }
        
        // For clang-cl we need to add the environment of cl.exe
        // to get a `works out of the box` experience

        Some(tool)
    }

    // Get the archiver for given target - used for static linking
    // msvc         -> lib.exe
    // android      -> {target.replace("armv7", "arm")}-ar
    // illumos      -> gar
    // emscripten
    //      windows
    //          cmd /c emar.bat
    //      other
    //          emar
    // other
    //      if host != target
    //          {target_prefix()}-ar OR {target_prefix()}-gcc-ar
    //      else
    //          gcc-ar OR ar
    fn archiver(target: &str, family: ToolFamily, from_path: bool) -> Option<Tool> {
        let default_ar = "ar".to_string();
        let program = if family.is_msvc() {            
            // We need a way to short circuit choosing a linker 
            // so test code can choose the mock executable
            if from_path {
                return Some(Tool::new_archiver("lib.exe"));
            } else {
                // For clang-cl we want to use `llvm-lib.exe`
                return msvc_tool("lib.exe", target);
            }
        } else if target.contains("android") {
            format!("{}-ar", target.replace("armv7", "arm"))
        } else if target.contains("emscripten") {
            return emscripten_tool("emar");
        } else if target.contains("illumos") {
            // The default 'ar' on illumos uses a non-standard flags,
            // but the OS comes bundled with a GNU-compatible variant.
            //
            // Use the GNU-variant to match other Unix systems.
            "gar".to_string()
        } else if host_triple() != target {
            // GCC uses $target-gcc-ar, whereas binutils uses $target-ar -- try both.
            // Prefer -ar if it exists, as builds of `-gcc-ar` have been observed to be
            // outright broken (such as when targetting freebsd with `--disable-lto`
            // toolchain where the archiver attempts to load the LTO plugin anyway but
            // fails to find one).
            let (gcc_ar, ar) = if let Some(prefix) = prefix_for_target(target, Language::C) {
                (format!("{}{}", prefix, "-gcc-ar"), format!("{}{}", prefix, "-ar"))
            } else {
                ("gcc-ar".to_string(), default_ar.clone())
            };
            let order = if family.is_gnu() { 
                [gcc_ar, ar, default_ar]
            } else {
                [ar, default_ar, gcc_ar]
            };
            return Some(Tool::new_archiver(
                order
                    .into_iter()
                    .filter_map(which)
                    .next()?
            ));
        } else if family.is_gnu() {
            // Linking LTO objects sometimes fails when not using gcc-ar, 
            // so we prefer it for native compilation
            return Some(Tool::new_archiver(which("gcc-ar").or_else(|| which("ar"))?));
        } else {
            default_ar
        };
        Some(Tool::new_archiver(which(program)?))
    }

    // Add required flags for target architecture for msvc
    fn add_target_flags_msvc(&mut self, target: &str) -> Result<(), Error> {
        // This is an undocumented flag from MSVC but helps with making
        // builds more reproducible by avoiding putting timestamps into
        // files.
        self.arg("-Brepro");

        // if clang_cl {
        //     if target.contains("x86_64") {
        //         self.arg("-m64");
        //     } else if target.contains("86") {
        //         self.arg("-m32");
        //         self.arg("-arch:IA32");
        //     } else {
        //         self.arg(format!("--target={}", target));
        //     }
        // } else {
            if target.contains("i586") {
                self.arg("-arch:IA32");
            }
        // }

        // There is a check in corecrt.h that will generate a
        // compilation error if
        // _ARM_WINAPI_PARTITION_DESKTOP_SDK_AVAILABLE is
        // not defined to 1. The check was added in Windows
        // 8 days because only store apps were allowed on ARM.
        // This changed with the release of Windows 10 IoT Core.
        // The check will be going away in future versions of
        // the SDK, but for all released versions of the
        // Windows SDK it is required.
        if target.contains("arm") || target.contains("thumb") {
            self.args.push("-D_ARM_WINAPI_PARTITION_DESKTOP_SDK_AVAILABLE=1".into());
        }
        Ok(())
    }
    
    // Add required flags for target architecture for clang
    fn add_target_flags_clang(&mut self, target: &str) -> Result<(), Error> {
        if target.contains("android") && android_clang_compiler_uses_target_arg_internally(self.path()) {
            return Ok(());
        }
        if target.contains("darwin") {
            if let Some(arch) = map_darwin_target_from_rust_to_compiler_architecture(target) {
                self.arg(format!("--target={}-apple-darwin", arch));
            }
        } else if target.contains("macabi") {
            if let Some(arch) = map_darwin_target_from_rust_to_compiler_architecture(target) {
                self.arg(format!("--target={}-apple-ios-macabi", arch));
            }
        } else if target.contains("ios-sim") {
            if let Some(arch) = map_darwin_target_from_rust_to_compiler_architecture(target) {
                let deployment_target = std::env::var("IPHONEOS_DEPLOYMENT_TARGET")
                    .unwrap_or_else(|_| "7.0".into());
                self.arg(
                    format!(
                        "--target={}-apple-ios{}-simulator",
                        arch, deployment_target
                    )
                );
            }
        } else if target.contains("watchos-sim") {
            if let Some(arch) = map_darwin_target_from_rust_to_compiler_architecture(target) {
                let deployment_target = std::env::var("WATCHOS_DEPLOYMENT_TARGET")
                    .unwrap_or_else(|_| "5.0".into());
                self.arg(
                    format!(
                        "--target={}-apple-watchos{}-simulator",
                        arch, deployment_target
                    )
                );
            }
        } else if target.starts_with("riscv64gc-") {
            self.arg(
                format!("--target={}", target.replace("riscv64gc", "riscv64"))
            );
        } else if target.starts_with("riscv32gc-") {
            self.arg(
                format!("--target={}", target.replace("riscv32gc", "riscv32"))
            );
        } else if target.contains("uefi") {
            if target.contains("x86_64") {
                self.arg("--target=x86_64-unknown-windows-gnu");
            } else if target.contains("i686") {
                self.arg("--target=i686-unknown-windows-gnu");
            } else if target.contains("aarch64") {
                self.arg("--target=aarch64-unknown-windows-gnu");
            }
        } else {
            self.arg(format!("--target={}", target));
        }
        Ok(())
    }
    
    // Add required flags for target architecture for gcc
    fn add_target_flags_gcc(&mut self, target: &str) -> Result<(), Error> {
        if target.contains("i686") || target.contains("i586") {
            self.arg("-m32");
        } else if target == "x86_64-unknown-linux-gnux32" {
            self.arg("-mx32");
        } else if target.contains("x86_64") || target.contains("powerpc64") {
            self.arg("-m64");
        }

        if target.contains("darwin") {
            if let Some(arch) = map_darwin_target_from_rust_to_compiler_architecture(target) {
                self.arg("-arch");
                self.arg(arch);
            }
        }

        if target.contains("-kmc-solid_") {
            self.arg("-finput-charset=utf-8");
        }

        // armv7 targets get to use armv7 instructions
        if (target.starts_with("armv7") || target.starts_with("thumbv7"))
            && (target.contains("-linux-") || target.contains("-kmc-solid_"))
        {
            self.arg("-march=armv7-a");

            if target.ends_with("eabihf") {
                // lowest common denominator FPU
                self.arg("-mfpu=vfpv3-d16");
            }
        }

        // (x86 Android doesn't say "eabi")
        if target.contains("-androideabi") && target.contains("v7") {
            // -march=armv7-a handled above
            self.arg("-mthumb");
            if !target.contains("neon") {
                // On android we can guarantee some extra float instructions
                // (specified in the android spec online)
                // NEON guarantees even more; see below.
                self.arg("-mfpu=vfpv3-d16");
            }
            self.arg("-mfloat-abi=softfp");
        }

        if target.contains("neon") {
            self.arg("-mfpu=neon-vfpv4");
        }

        if target.starts_with("armv4t-unknown-linux-") {
            self.arg("-march=armv4t");
            self.arg("-marm");
            self.arg("-mfloat-abi=soft");
        }

        if target.starts_with("armv5te-unknown-linux-") {
            self.arg("-march=armv5te");
            self.arg("-marm");
            self.arg("-mfloat-abi=soft");
        }

        // For us arm == armv6 by default
        if target.starts_with("arm-unknown-linux-") {
            self.arg("-march=armv6");
            self.arg("-marm");
            if target.ends_with("hf") {
                self.arg("-mfpu=vfp");
            } else {
                self.arg("-mfloat-abi=soft");
            }
        }

        // We can guarantee some settings for FRC
        if target.starts_with("arm-frc-") {
            self.arg("-march=armv7-a");
            self.arg("-mcpu=cortex-a9");
            self.arg("-mfpu=vfpv3");
            self.arg("-mfloat-abi=softfp");
            self.arg("-marm");
        }

        // Turn codegen down on i586 to avoid some instructions.
        if target.starts_with("i586-unknown-linux-") {
            self.arg("-march=pentium");
        }

        // Set codegen level for i686 correctly
        if target.starts_with("i686-unknown-linux-") {
            self.arg("-march=i686");
        }

        // Looks like `musl-gcc` makes it hard for `-m32` to make its way
        // all the way to the linker, so we need to actually instruct the
        // linker that we're generating 32-bit executables as well. This'll
        // typically only be used for build scripts which transitively use
        // these flags that try to compile executables.
        if target == "i686-unknown-linux-musl" || target == "i586-unknown-linux-musl" {
            self.arg("-Wl,-melf_i386");
        }

        if target.starts_with("thumb") {
            self.arg("-mthumb");

            if target.ends_with("eabihf") {
                self.arg("-mfloat-abi=hard");
            }
        }
        if target.starts_with("thumbv6m") {
            self.arg("-march=armv6s-m");
        }
        if target.starts_with("thumbv7em") {
            self.arg("-march=armv7e-m");

            if target.ends_with("eabihf") {
                self.arg("-mfpu=fpv4-sp-d16");
            }
        }
        if target.starts_with("thumbv7m") {
            self.arg("-march=armv7-m");
        }
        if target.starts_with("thumbv8m.base") {
            self.arg("-march=armv8-m.base");
        }
        if target.starts_with("thumbv8m.main") {
            self.arg("-march=armv8-m.main");

            if target.ends_with("eabihf") {
                self.arg("-mfpu=fpv5-sp-d16");
            }
        }
        if target.starts_with("armebv7r") | target.starts_with("armv7r") {
            if target.starts_with("armeb") {
                self.arg("-mbig-endian");
            } else {
                self.arg("-mlittle-endian");
            }

            // ARM mode
            self.arg("-marm");

            // R Profile
            self.arg("-march=armv7-r");

            if target.ends_with("eabihf") {
                // Calling convention
                self.arg("-mfloat-abi=hard");

                // lowest common denominator FPU
                // (see Cortex-R4 technical reference manual)
                self.arg("-mfpu=vfpv3-d16");
            } else {
                // Calling convention
                self.arg("-mfloat-abi=soft");
            }
        }
        if target.starts_with("armv7a") {
            self.arg("-march=armv7-a");

            if target.ends_with("eabihf") {
                // lowest common denominator FPU
                self.arg("-mfpu=vfpv3-d16");
            }
        }
        if target.starts_with("riscv32") || target.starts_with("riscv64") {
            // get the 32i/32imac/32imc/64gc/64imac/... part
            let mut parts = target.split('-');
            if let Some(arch) = parts.next() {
                let arch = &arch[5..];
                if target.contains("linux") && arch.starts_with("64") {
                    self.arg("-march=rv64gc");
                    self.arg("-mabi=lp64d");
                } else if target.contains("freebsd") && arch.starts_with("64") {
                    self.arg("-march=rv64gc");
                    self.arg("-mabi=lp64d");
                } else if target.contains("openbsd") && arch.starts_with("64") {
                    self.arg("-march=rv64gc");
                    self.arg("-mabi=lp64d");
                } else if target.contains("linux") && arch.starts_with("32") {
                    self.arg("-march=rv32gc");
                    self.arg("-mabi=ilp32d");
                } else if arch.starts_with("64") {
                    self.arg("-march=rv".to_owned() + arch);
                    self.arg("-mabi=lp64");
                } else {
                    self.arg("-march=rv".to_owned() + arch);
                    self.arg("-mabi=ilp32");
                }
                self.arg("-mcmodel=medany");
            }
        }
        Ok(())
    }

    // Add required flags for Apple IOS/WatchOS targets
    fn add_ios_watchos_flags(&mut self, target: &str) -> Result<(), Error> {
        enum ArchSpec {
            Device(&'static str),
            Simulator(&'static str),
            Catalyst(&'static str),
        }
        #[derive(Debug)]
        enum Os {
            Ios,
            WatchOs,
        }

        let os = if target.contains("-watchos") {
            Os::WatchOs
        } else {
            Os::Ios
        };

        let arch = target.split('-').nth(0).ok_or_else(|| {
            Error::invalid_arch(format!("Unknown architecture for {:?} target.", os))
        })?;

        let is_catalyst = match target.split('-').nth(3) {
            Some(v) => v == "macabi",
            None => false,
        };

        let is_sim = match target.split('-').nth(3) {
            Some(v) => v == "sim",
            None => false,
        };

        let arch = if is_catalyst {
            match arch {
                "arm64e" => ArchSpec::Catalyst("arm64e"),
                "arm64" | "aarch64" => ArchSpec::Catalyst("arm64"),
                "x86_64" => ArchSpec::Catalyst("-m64"),
                _ => {
                    return Err(Error::invalid_arch("Unknown architecture for iOS target."));
                }
            }
        } else if is_sim {
            match arch {
                "arm64" | "aarch64" => ArchSpec::Simulator("-arch arm64"),
                "x86_64" => ArchSpec::Simulator("-m64"),
                _ => {
                    return Err(Error::invalid_arch("Unknown architecture for iOS simulator target."));
                }
            }
        } else {
            match arch {
                "arm" | "armv7" | "thumbv7" => ArchSpec::Device("armv7"),
                "armv7k" => ArchSpec::Device("armv7k"),
                "armv7s" | "thumbv7s" => ArchSpec::Device("armv7s"),
                "arm64e" => ArchSpec::Device("arm64e"),
                "arm64" | "aarch64" => ArchSpec::Device("arm64"),
                "arm64_32" => ArchSpec::Device("arm64_32"),
                "i386" | "i686" => ArchSpec::Simulator("-m32"),
                "x86_64" => ArchSpec::Simulator("-m64"),
                _ => {
                    return Err(Error::invalid_arch(format!("Unknown architecture for {:?} target.", os)));
                }
            }
        };

        let (sdk_prefix, sim_prefix, min_version) = match os {
            Os::Ios => (
                "iphone",
                "ios-",
                std::env::var("IPHONEOS_DEPLOYMENT_TARGET").unwrap_or_else(|_| "7.0".into()),
            ),
            Os::WatchOs => (
                "watch",
                "watch",
                std::env::var("WATCHOS_DEPLOYMENT_TARGET").unwrap_or_else(|_| "2.0".into()),
            ),
        };

        let sdk = match arch {
            ArchSpec::Device(arch) => {
                self.arg("-arch");
                self.arg(arch);
                self.arg(format!("-m{}os-version-min={}", sdk_prefix, min_version));
                format!("{}os", sdk_prefix)
            }
            ArchSpec::Simulator(arch) => {
                self.arg(arch);
                self.arg(format!("-m{}simulator-version-min={}", sim_prefix, min_version));
                format!("{}simulator", sdk_prefix)
            }
            ArchSpec::Catalyst(_) => "macosx".to_owned(),
        };

        let sdk_path = if let Some(sdkroot) = std::env::var_os("SDKROOT") {
            sdkroot
        } else {
            apple_sdk_root(sdk.as_str())?
        };

        self.arg("-isysroot");
        self.args.push(sdk_path);
        self.arg("-fembed-bitcode");
        /*
        * TODO we probably ultimately want the -fembed-bitcode-marker flag
        * but can't have it now because of an issue in LLVM:
        * https://github.com/rust-lang/cc-rs/issues/301
        * https://github.com/rust-lang/rust/pull/48896#comment-372192660
        */
        /*
        if self.get_opt_level()? == "0" {
            self.arg("-fembed-bitcode-marker");
        }
        */
        Ok(())
    }

    // Ensures `SDKROOT` environment variable is correctly set on Apple systems
    fn fix_env_for_apple_os(&mut self, target: &str) -> Result<(), Error> {
        if host_triple().contains("apple-darwin") && target.contains("apple-darwin") {
            // If, for example, `ccargo` runs during the build of an XCode project, then `SDKROOT` environment variable
            // would represent the current target, and this is the problem for us, if we want to compile something
            // for the host, when host != target.
            // We can not just remove `SDKROOT`, because, again, for example, XCode add to PATH
            // /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin
            // and `cc` from this path can not find system include files, like `pthread.h`, if `SDKROOT`
            // is not set
            if let Ok(sdkroot) = std::env::var("SDKROOT") {
                if !sdkroot.contains("MacOSX") {
                    let macos_sdk = apple_sdk_root("macosx")?;
                    self.env.push(("SDKROOT".into(), macos_sdk));
                }
            }
            // Additionally, `IPHONEOS_DEPLOYMENT_TARGET` must not be set when using the Xcode linker at
            // "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/ld",
            // although this is apparently ignored when using the linker at "/usr/bin/ld".
            self.env_remove.push("IPHONEOS_DEPLOYMENT_TARGET".into());
        }
        Ok(())
    }
}


impl std::hash::Hash for Tool {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.family.hash(state);
        self.path.hash(state);
    }
}

impl fmt::Debug for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We only care about the kind/family/path when debugging a tool, the rest can be printed manually
        // Otherwise we get a giant spew of args/environment variables that cannot be read
        f.debug_struct("Tool")
            .field("kind", &self.kind)
            .field("family", &self.family)    
            .field("path", &self.path)
            .finish()
    }
}


// Find an executable in the operating system's environment paths
pub fn which(exe: impl AsRef<Path>) -> Option<PathBuf> {
    let exe = exe.as_ref();
    
    // If |tool| is not just one "word," assume it's an actual path...
    if exe.components().count() > 1 {
        let path = PathBuf::from(exe);
        if path.exists() {
            return Some(path);
        }
    }

    // Loop through PATH entries searching for the |tool|.
    let path_entries = std::env::var_os("PATH")?;
    std::env::split_paths(&path_entries).find_map(|path_entry| {
        let mut path = path_entry.join(exe);
        path.set_extension(std::env::consts::EXE_EXTENSION);
        if path.exists() {
            return Some(path)
        }
        None
    })
}


// Parse the version from the output of a tool's version information
fn parse_version(family: ToolFamily, out: &str) -> Result<Version, Error> {
    let version = match family {
        ToolFamily::Msvc => {
            // Between `Version ` and next whitespace
            let begin = out.find("Version ").unwrap();
            let rest = &out[begin + "Version ".len()..];
            let end = rest.find(' ').unwrap();
            &rest[..end]
        }
        ToolFamily::Clang => {
            // Third token when splitting whitespace
            let mut s = out.split_whitespace();
            drop(s.next());
            drop(s.next());
            s.next().unwrap()
        }
        ToolFamily::Gnu => {
            // Last token on first line when splitting whitespace
            let line = out.lines().next().unwrap();
            line.split_whitespace().last().unwrap()
        }
    };

    match Version::parse(version) {
        Ok(r) => Ok(r),
        Err(e) => Err(Error::tool_exec(format!(
            "Failed to parse version from `{version}`: {:?}", e
        )))
    }
}


// Create an msvc tool from the result of `cc::windows_registry::find_tool`
fn msvc_tool(tool: &str, target: &str) -> Option<Tool> {
    // Detect tool kind from name
    let kind = match tool {
        "cl.exe" => ToolKind::Compiler,
        "link.exe" => ToolKind::Linker,
        "lib.exe" => ToolKind::Archiver,
        _ => unreachable!(),
    };
    let cc_tool = cc::windows_registry::find_tool(target, tool)?;
    let mut tool = Tool::new(kind, cc_tool.path().to_path_buf());
    tool.args.extend(cc_tool.args().iter().cloned());
    tool.env.extend(cc_tool.env().iter().cloned());
    tool.family = ToolFamily::Msvc;
    Some(tool)
}


// Create an emscripten tool for both windows and other
fn emscripten_tool(tool: &str) -> Option<Tool> {
    // Windows use bat files so we have to be a bit more specific
    let kind = if tool == "emar" { ToolKind::Archiver } else { ToolKind::Compiler };
    Some(if cfg!(windows) {
        let mut t = Tool::new(kind, PathBuf::from("cmd"));
        t.arg("/c").arg(format!("{tool}.bat"));
        t
    } else {
        Tool::new(kind, which(tool)?)
    })
}


// Gnu prefix for cross-compiling to a given target
fn prefix_for_target(target: &str, language: Language) -> Option<&'static str> {
    match &target[..] {
        // Note: there is no `aarch64-pc-windows-gnu` target, only `-gnullvm`
        "aarch64-pc-windows-gnullvm" => Some("aarch64-w64-mingw32"),
        "aarch64-uwp-windows-gnu" => Some("aarch64-w64-mingw32"),
        "aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu"),
        "aarch64-unknown-linux-musl" => Some("aarch64-linux-musl"),
        "aarch64-unknown-netbsd" => Some("aarch64--netbsd"),
        "arm-unknown-linux-gnueabi" => Some("arm-linux-gnueabi"),
        "armv4t-unknown-linux-gnueabi" => Some("arm-linux-gnueabi"),
        "armv5te-unknown-linux-gnueabi" => Some("arm-linux-gnueabi"),
        "armv5te-unknown-linux-musleabi" => Some("arm-linux-gnueabi"),
        "arm-frc-linux-gnueabi" => Some("arm-frc-linux-gnueabi"),
        "arm-unknown-linux-gnueabihf" => Some("arm-linux-gnueabihf"),
        "arm-unknown-linux-musleabi" => Some("arm-linux-musleabi"),
        "arm-unknown-linux-musleabihf" => Some("arm-linux-musleabihf"),
        "arm-unknown-netbsd-eabi" => Some("arm--netbsdelf-eabi"),
        "armv6-unknown-netbsd-eabihf" => Some("armv6--netbsdelf-eabihf"),
        "armv7-unknown-linux-gnueabi" => Some("arm-linux-gnueabi"),
        "armv7-unknown-linux-gnueabihf" => Some("arm-linux-gnueabihf"),
        "armv7-unknown-linux-musleabihf" => Some("arm-linux-musleabihf"),
        "armv7neon-unknown-linux-gnueabihf" => Some("arm-linux-gnueabihf"),
        "armv7neon-unknown-linux-musleabihf" => Some("arm-linux-musleabihf"),
        "thumbv7-unknown-linux-gnueabihf" => Some("arm-linux-gnueabihf"),
        "thumbv7-unknown-linux-musleabihf" => Some("arm-linux-musleabihf"),
        "thumbv7neon-unknown-linux-gnueabihf" => Some("arm-linux-gnueabihf"),
        "thumbv7neon-unknown-linux-musleabihf" => Some("arm-linux-musleabihf"),
        "armv7-unknown-netbsd-eabihf" => Some("armv7--netbsdelf-eabihf"),
        "hexagon-unknown-linux-musl" => Some("hexagon-linux-musl"),
        "i586-unknown-linux-musl" => Some("musl"),
        "i686-pc-windows-gnu" => Some("i686-w64-mingw32"),
        "i686-uwp-windows-gnu" => Some("i686-w64-mingw32"),
        "i686-unknown-linux-gnu" => find_working_gnu_prefix(&[
            "i686-linux-gnu",
            "x86_64-linux-gnu", // transparently support gcc-multilib
        ], language), // explicit None if not found, so caller knows to fall back
        "i686-unknown-linux-musl" => Some("musl"),
        "i686-unknown-netbsd" => Some("i486--netbsdelf"),
        "mips-unknown-linux-gnu" => Some("mips-linux-gnu"),
        "mips-unknown-linux-musl" => Some("mips-linux-musl"),
        "mipsel-unknown-linux-gnu" => Some("mipsel-linux-gnu"),
        "mipsel-unknown-linux-musl" => Some("mipsel-linux-musl"),
        "mips64-unknown-linux-gnuabi64" => Some("mips64-linux-gnuabi64"),
        "mips64el-unknown-linux-gnuabi64" => Some("mips64el-linux-gnuabi64"),
        "mipsisa32r6-unknown-linux-gnu" => Some("mipsisa32r6-linux-gnu"),
        "mipsisa32r6el-unknown-linux-gnu" => Some("mipsisa32r6el-linux-gnu"),
        "mipsisa64r6-unknown-linux-gnuabi64" => Some("mipsisa64r6-linux-gnuabi64"),
        "mipsisa64r6el-unknown-linux-gnuabi64" => Some("mipsisa64r6el-linux-gnuabi64"),
        "powerpc-unknown-linux-gnu" => Some("powerpc-linux-gnu"),
        "powerpc-unknown-linux-gnuspe" => Some("powerpc-linux-gnuspe"),
        "powerpc-unknown-netbsd" => Some("powerpc--netbsd"),
        "powerpc64-unknown-linux-gnu" => Some("powerpc-linux-gnu"),
        "powerpc64le-unknown-linux-gnu" => Some("powerpc64le-linux-gnu"),
        "riscv32i-unknown-none-elf" => find_working_gnu_prefix(&[
            "riscv32-unknown-elf",
            "riscv64-unknown-elf",
            "riscv-none-embed",
        ], language),
        "riscv32imac-unknown-none-elf" => find_working_gnu_prefix(&[
            "riscv32-unknown-elf",
            "riscv64-unknown-elf",
            "riscv-none-embed",
        ], language),
        "riscv32imac-unknown-xous-elf" => find_working_gnu_prefix(&[
            "riscv32-unknown-elf",
            "riscv64-unknown-elf",
            "riscv-none-embed",
        ], language),
        "riscv32imc-unknown-none-elf" => find_working_gnu_prefix(&[
            "riscv32-unknown-elf",
            "riscv64-unknown-elf",
            "riscv-none-embed",
        ], language),
        "riscv64gc-unknown-none-elf" => find_working_gnu_prefix(&[
            "riscv64-unknown-elf",
            "riscv32-unknown-elf",
            "riscv-none-embed",
        ], language),
        "riscv64imac-unknown-none-elf" => find_working_gnu_prefix(&[
            "riscv64-unknown-elf",
            "riscv32-unknown-elf",
            "riscv-none-embed",
        ], language),
        "riscv64gc-unknown-linux-gnu" => Some("riscv64-linux-gnu"),
        "riscv32gc-unknown-linux-gnu" => Some("riscv32-linux-gnu"),
        "riscv64gc-unknown-linux-musl" => Some("riscv64-linux-musl"),
        "riscv32gc-unknown-linux-musl" => Some("riscv32-linux-musl"),
        "s390x-unknown-linux-gnu" => Some("s390x-linux-gnu"),
        "sparc-unknown-linux-gnu" => Some("sparc-linux-gnu"),
        "sparc64-unknown-linux-gnu" => Some("sparc64-linux-gnu"),
        "sparc64-unknown-netbsd" => Some("sparc64--netbsd"),
        "sparcv9-sun-solaris" => Some("sparcv9-sun-solaris"),
        "armv7a-none-eabi" => Some("arm-none-eabi"),
        "armv7a-none-eabihf" => Some("arm-none-eabi"),
        "armebv7r-none-eabi" => Some("arm-none-eabi"),
        "armebv7r-none-eabihf" => Some("arm-none-eabi"),
        "armv7r-none-eabi" => Some("arm-none-eabi"),
        "armv7r-none-eabihf" => Some("arm-none-eabi"),
        "thumbv6m-none-eabi" => Some("arm-none-eabi"),
        "thumbv7em-none-eabi" => Some("arm-none-eabi"),
        "thumbv7em-none-eabihf" => Some("arm-none-eabi"),
        "thumbv7m-none-eabi" => Some("arm-none-eabi"),
        "thumbv8m.base-none-eabi" => Some("arm-none-eabi"),
        "thumbv8m.main-none-eabi" => Some("arm-none-eabi"),
        "thumbv8m.main-none-eabihf" => Some("arm-none-eabi"),
        "x86_64-pc-windows-gnu" => Some("x86_64-w64-mingw32"),
        "x86_64-pc-windows-gnullvm" => Some("x86_64-w64-mingw32"),
        "x86_64-uwp-windows-gnu" => Some("x86_64-w64-mingw32"),
        "x86_64-rumprun-netbsd" => Some("x86_64-rumprun-netbsd"),
        "x86_64-unknown-linux-gnu" => find_working_gnu_prefix(&[
            "x86_64-linux-gnu", // rustfmt wrap
        ], language), // explicit None if not found, so caller knows to fall back
        "x86_64-unknown-linux-musl" => Some("musl"),
        "x86_64-unknown-netbsd" => Some("x86_64--netbsd"),
        _ => None,
    }
}


/// Some platforms have multiple, compatible, canonical prefixes. Look through
/// each possible prefix for a compiler that exists and return it. The prefixes
/// should be ordered from most-likely to least-likely.
fn find_working_gnu_prefix(prefixes: &[&'static str], language: Language) -> Option<&'static str> {
    let suffix = if language.is_cxx() { "-g++" } else { "-gcc" };
    let path = prefixes
        .iter()
        .map(|v| format!("{}{}", v, suffix))
        .filter_map(which)
        .next()?;
    let fname = path.file_name()?.to_str()?;
    prefixes.iter().find(|v| fname.contains(*v)).copied()
}


// Automatically detect and return corresponding android compiler for given target and host
fn autodetect_android_compiler(target: &str, host: &str, gnu: &str, clang: &str) -> String {
    // Use by default minimum available API level
    // See note about naming here
    // https://android.googlesource.com/platform/ndk/+/refs/heads/ndk-release-r21/docs/BuildSystemMaintainers.md#Clang
    static NEW_STANDALONE_ANDROID_COMPILERS: [&str; 4] = [
        "aarch64-linux-android21-clang",
        "armv7a-linux-androideabi16-clang",
        "i686-linux-android16-clang",
        "x86_64-linux-android21-clang",
    ];

    let new_clang_key = match target {
        "aarch64-linux-android" => Some("aarch64"),
        "armv7-linux-androideabi" => Some("armv7a"),
        "i686-linux-android" => Some("i686"),
        "x86_64-linux-android" => Some("x86_64"),
        _ => None,
    };

    let new_clang = new_clang_key
        .map(|key| {
            NEW_STANDALONE_ANDROID_COMPILERS
                .iter()
                .find(|x| x.starts_with(key))
        })
        .unwrap_or(None);

    if let Some(new_clang) = new_clang {
        if which(new_clang).is_some() {
            return (*new_clang).into();
        }
    }

    let target = target
        .replace("armv7neon", "arm")
        .replace("armv7", "arm")
        .replace("thumbv7neon", "arm")
        .replace("thumbv7", "arm");
    let gnu_compiler = format!("{}-{}", target, gnu);
    let clang_compiler = format!("{}-{}", target, clang);

    // On Windows, the Android clang compiler is provided as a `.cmd` file instead
    // of a `.exe` file. `std::process::Command` won't run `.cmd` files unless the
    // `.cmd` is explicitly appended to the command name, so we do that here.
    let clang_compiler_cmd = format!("{}-{}.cmd", target, clang);

    // Check if gnu compiler is present
    // if not, use clang
    if which(&gnu_compiler).is_some() {
        gnu_compiler
    } else if host.contains("windows") && which(&clang_compiler_cmd).is_some() {
        clang_compiler_cmd
    } else {
        clang_compiler
    }
}


// New "standalone" C/C++ cross-compiler executables from recent Android NDK
// are just shell scripts that call main clang binary (from Android NDK) with
// proper `--target` argument.
//
// For example, armv7a-linux-androideabi16-clang passes
// `--target=armv7a-linux-androideabi16` to clang.
// So to construct proper command line check if
// `--target` argument would be passed or not to clang
fn android_clang_compiler_uses_target_arg_internally(clang_path: &Path) -> bool {
    if let Some(filename) = clang_path.file_name() {
        if let Some(filename_str) = filename.to_str() {
            filename_str.contains("android")
        } else {
            false
        }
    } else {
        false
    }
}


// New "standalone" C/C++ cross-compiler executables from recent Android NDK
// are just shell scripts that call main clang binary (from Android NDK) with
// proper `--target` argument.
//
// For example, armv7a-linux-androideabi16-clang passes
// `--target=armv7a-linux-androideabi16` to clang.
//
// As the shell script calls the main clang binary, the command line limit length
// on Windows is restricted to around 8k characters instead of around 32k characters.
// To remove this limit, we call the main clang binary directly and construct the
// `--target=` ourselves.
fn android_get_default_clang_compiler_target_flag(tool: &mut Tool) {
    if let Some(path) = tool.path.file_name() {
        let file_name = path.to_str().unwrap().to_owned();
        let (target, clang) = file_name.split_at(file_name.rfind("-").unwrap());

        tool.path.set_file_name(clang.trim_start_matches("-"));
        tool.path.set_extension("exe");
        tool.args.push(format!("--target={}", target).into());

        // Additionally, shell scripts for target i686-linux-android versions 16 to 24
        // pass the `mstackrealign` option so we do that here as well.
        if target.contains("i686-linux-android") {
            let (_, version) = target.split_at(target.rfind("d").unwrap() + 1);
            if let Ok(version) = version.parse::<u32>() {
                if version > 15 && version < 25 {
                    tool.args.push("-mstackrealign".into());
                }
            }
        }
    };
}


// Rust and clang/cc don't agree on how to name the target.
fn map_darwin_target_from_rust_to_compiler_architecture(target: &str) -> Option<&'static str> {
    if target.contains("x86_64") {
        Some("x86_64")
    } else if target.contains("arm64e") {
        Some("arm64e")
    } else if target.contains("aarch64") {
        Some("arm64")
    } else if target.contains("i686") {
        Some("i386")
    } else if target.contains("powerpc") {
        Some("ppc")
    } else if target.contains("powerpc64") {
        Some("ppc64")
    } else {
        None
    }
}


// Add env var to `ar` invocation on macos to make builds more reproducible
fn macos_ar_ensure_file_times_are_zero(cmd: &mut Command) {
    // Set an environment variable to tell the OSX archiver to ensure
    // that all dates listed in the archive are zero, improving
    // determinism of builds. AFAIK there's not really official
    // documentation of this but there's a lot of references to it if
    // you search google.
    //
    // You can reproduce this locally on a mac with:
    //
    //      $ touch foo.c
    //      $ cc -c foo.c -o foo.o
    //
    //      # Notice that these two checksums are different
    //      $ ar crus libfoo1.a foo.o && sleep 2 && ar crus libfoo2.a foo.o
    //      $ md5sum libfoo*.a
    //
    //      # Notice that these two checksums are the same
    //      $ export ZERO_AR_DATE=1
    //      $ ar crus libfoo1.a foo.o && sleep 2 && touch foo.o && ar crus libfoo2.a foo.o
    //      $ md5sum libfoo*.a
    //
    // In any case if this doesn't end up getting read, it shouldn't
    // cause that many issues!
    cmd.env("ZERO_AR_DATE", "1");
}


// Detect root directory of the given SDK using `xcrun --show-sdk-path --sdk $sdk`
fn apple_sdk_root(sdk: &str) -> Result<OsString, Error> {
    let mut cache = APPLE_SDK_CACHE.lock().unwrap();
    if let Some(ret) = cache.get(sdk) {
        return Ok(ret.clone());
    }

    let sdk_path = run_stdout(
        Command::new("xcrun")
            .arg("--show-sdk-path")
            .arg("--sdk")
            .arg(sdk),
        "xcrun",
    )?;

    let sdk_path = match String::from_utf8(sdk_path) {
        Ok(p) => p,
        Err(_) => return Err(Error::io("Unable to determine Apple SDK path.")),
    };
    let ret: OsString = sdk_path.trim().into();
    cache.insert(sdk.into(), ret.clone());
    Ok(ret)
}

// Cache of apple SDK paths to prevent running `xcrun` multiple times
lazy_static::lazy_static! {
    static ref APPLE_SDK_CACHE: Mutex<HashMap<String, OsString>> = Mutex::new(HashMap::new());
}
