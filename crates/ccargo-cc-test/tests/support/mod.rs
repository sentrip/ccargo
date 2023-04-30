#![allow(dead_code)]

use ccargo::cc::*;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::io::{self, prelude::*};
use std::process::Command;
use tempfile::{TempDir, Builder};


// Macros for optionally compiling tests based on the availability of the requested tool family
include!(concat!(env!("OUT_DIR"), "/tool_macros.rs"));

// Root path of binaries compiled at build time in `build.rs`
pub fn exes_root() -> &'static Path {
    &Path::new(env!("OUT_DIR"))
}

// Run command and return status code
pub fn exe_status(path: &Path) -> Option<i32> {
    if let Ok(r) = Command::new(path).status() {
        r.code()
    } else {
        None
    }
}
 


#[derive(Debug)]
pub struct Test {
    pub td: TempDir,
    pub gcc: PathBuf,
    pub msvc: bool,
}

#[derive(Debug)]
pub struct Execution {
    args: Vec<String>,
}

impl Test {
    pub fn new() -> Test {
        let mut gcc = PathBuf::from(env::current_exe().unwrap());
        gcc.pop();
        if gcc.ends_with("deps") {
            gcc.pop();
        }
        let td = Builder::new().prefix("gcc-test").tempdir_in(&gcc).unwrap();
        gcc.push(format!("gcc-shim{}", env::consts::EXE_SUFFIX));
        Test {
            td: td,
            gcc: gcc,
            msvc: false,
        }
    }
    
    pub fn gnu() -> Test {
        let t = Test::new();
        t.shim("cc").shim("c++").shim("ar");
        t
    }

    pub fn msvc() -> Test {
        let mut t = Test::new();
        t.shim("cl.exe").shim("lib.exe").shim("link.exe");
        t.msvc = true;
        t
    }

    pub fn shim(&self, name: &str) -> &Test {
        let name = if name.ends_with(env::consts::EXE_SUFFIX) {
            name.to_string()
        } else {
            format!("{}{}", name, env::consts::EXE_SUFFIX)
        };
        link_or_copy(&self.gcc, self.td.path().join(name)).unwrap();
        self
    }

    pub fn cc(&self, bin_type: BinType) -> Build {
        let target = if self.msvc {
            "x86_64-pc-windows-msvc"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        let (cc, cxx) = if self.msvc {
            ("cl.exe", "cl.exe")
        } else {
            ("cc", "c++")
        };
        let path = self.td.path();
        let tc = Toolchain::new(target, path.join(cc), path.join(cxx)).unwrap();
        let mut b = Build::new("foo", bin_type, tc);
        b.out_dir(path)
            .profile(Profile::dev())
            .__set_env("GCCTEST_OUT_DIR", self.td.path());
        b
    }

    pub fn cmd(&self, i: u32) -> Execution {
        let mut s = String::new();
        File::open(self.td.path().join(format!("out{}", i)))
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        Execution {
            args: s.lines().map(|s| s.to_string()).collect(),
        }
    }
}

impl Execution {
    pub fn must_have<P: AsRef<OsStr>>(&self, p: P) -> &Execution {
        if !self.has(p.as_ref()) {
            panic!("didn't find {:?} in {:?}", p.as_ref(), self.args);
        } else {
            self
        }
    }

    pub fn must_not_have<P: AsRef<OsStr>>(&self, p: P) -> &Execution {
        if self.has(p.as_ref()) {
            panic!("found {:?}", p.as_ref());
        } else {
            self
        }
    }

    pub fn print(&self) -> &Execution {
        println!("{:?}", self.args);
        self
    }

    pub fn has(&self, p: &OsStr) -> bool {
        self.args.iter().any(|arg| OsStr::new(arg) == p)
    }
}

/// Hard link an executable or copy it if that fails.
///
/// We first try to hard link an executable to save space. If that fails (as on Windows with
/// different mount points, issue #60), we copy.
#[cfg(not(target_os = "macos"))]
fn link_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs::hard_link(from, to).or_else(|_| fs::copy(from, to).map(|_| ()))
}

/// Copy an executable.
///
/// On macOS, hard linking the executable leads to strange failures (issue #419), so we just copy.
#[cfg(target_os = "macos")]
fn link_or_copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    fs::copy(from, to).map(|_| ())
}

