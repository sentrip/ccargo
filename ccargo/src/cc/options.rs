use crate::utils::InternedString;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use serde::{Serialize, Deserialize};


// Set of user-provided compiler/linker/archiver flags
pub type FlagSet = BTreeSet<String>;


// User-configurable options for controlling the build process
#[derive(Debug, Clone, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Options {
    // C/C++ standard
    pub std: Std,
    // C runtime libraries used for linking
    pub crt: Crt,
    // warnings
    pub warnings: Warnings,
    // compiler defines provided during compilation
    pub defines: FlagSet,
    // flags passed to the compiler
    pub cc_flags: FlagSet,
    // flags passed to the linker
    pub ld_flags: FlagSet,
    // flags passed to the archiver
    pub ar_flags: FlagSet,
    // flags used when compiling assembly files
    pub asm_flags: FlagSet,
    // flags for unix-like targets
    pub unix: UnixFlags,
}


// Warnings
#[derive(Debug, Clone, Hash, Default, Serialize, Deserialize)]
pub struct Warnings {
    // warning level
    pub level: WarningLevel,
    // treat warnings as errors
    pub errors: bool,
    // extra platform-specific warning flags
    pub extra: FlagSet,
}


// Flags that are only supported on unix-like targets
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UnixFlags {
    pub pic: bool,
    pub plt: bool,
    pub force_frame_pointer: bool,
}
impl std::default::Default for UnixFlags {
    fn default() -> Self { Self { pic: true, plt: true, force_frame_pointer: false } }
}


// C standard
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StdC {
    C89,
    C99,
    #[default]
    C11,
    C17,
    C20,
}


// C++ standard
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub enum StdCxx {
    #[serde(rename = "c++98")]
    Cxx98,
    #[serde(rename = "c++11")]
    Cxx11,
    #[serde(rename = "c++14")]
    Cxx14,
    #[default]
    #[serde(rename = "c++17")]
    Cxx17,
    #[serde(rename = "c++20")]
    Cxx20,
}


// C/C++ standards
#[derive(Debug, Clone, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Std {
    pub c: StdC,
    pub cxx: StdCxx,
    pub cxx_stdlib: Option<String>,
    pub gnu: bool,
}


// C runtime library
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Crt {
    #[default]
    Default,
    Static,
    Shared,
}


// Represents a generic warning level used to select different sets of compiler warnings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningLevel {
    // no warnings - not recommended
    None,
    // some warnings (/W3 OR -Wall)
    #[default]
    Default,
    // most warnings (/W4 OR -Wall -Wextra -Wpedantic)
    Extra,
    // all warnings (/Wall OR -Wall -Wextra -Wpedantic -Wconversion ...)
    All,
}


// Link-time optimization performed on target
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lto {
    Off,
    #[default]
    Thin,
    Fat
}


// Represents a generic optimization level used to select different compiler optimizations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub enum OptLevel {
    #[default]
    // no optimizations
    O0,
    // basic optimizations
    O1,
    // some optimizations
    O2,
    // all optimizations
    O3,
    // optimize for binary size
    Os,
    // optimize for binary size, but also turn off loop vectorization.
    Oz,
}

// Source language (C/C++)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "c")]
    C,
    #[default]
    #[serde(rename = "c++")]
    Cxx,
}

impl Language {
    pub fn is_c(self) -> bool { self == Self::C }
    pub fn is_cxx(self) -> bool { self == Self::Cxx }

    // Detect language from path - treats `*.c` as C files, and the rest as C++ files
    pub fn detect<P: AsRef<Path>>(path: P) -> Self {
        if path.as_ref().extension()
            .map(|v| v == "c" || v == "S" || v == "asm")
            .unwrap_or(false) 
        { 
            Self::C
        } else { 
            Self::Cxx
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::C => "c",
            Self::Cxx => "c++"
        }.fmt(f)
    }
}


/// A profile used to configure how a target is compiled
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Profile {
    // generate debug information
    pub debug: bool,
    // link target incrementally
    pub incremental: bool,
    // use exceptions
    pub exceptions: bool,
    // optimization level
    pub opt_level: OptLevel,
    // link-time optimization
    pub lto: Lto,
    // output directory inside `target`
    pub dir_name: InternedString,    
    // rpath used for dynamic linking on platforms that support it (default is $ORIGIN)
    pub rpath: InternedString,
}

impl Profile {
    pub fn is_optimized(&self) -> bool {
        self.opt_level > OptLevel::O0
    }
    
    pub fn is_incremental(&self) -> bool {
        self.incremental && !self.is_optimized()
    }

    pub fn is_lto_enabled(&self) -> bool {
        self.is_optimized() && self.lto != Lto::Off
    }

    pub fn dev() -> Self {
        Self {
            debug: true,
            incremental: true,
            exceptions: true,
            opt_level: OptLevel::O0,
            lto: Lto::Off,
            dir_name: InternedString::from("debug"),
            rpath: InternedString::from("$ORIGIN"),
        }
    }

    pub fn release() -> Self {
        Self {
            debug: false,
            incremental: false,
            exceptions: true,
            opt_level: OptLevel::O3,
            lto: Lto::Fat,
            dir_name: InternedString::from("release"),
            rpath: InternedString::from("$ORIGIN"),
        }
    }
}
