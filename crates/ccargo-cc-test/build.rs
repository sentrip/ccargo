/// There are 2 things this file aims to achieve:
///     - Compiling test binaries from sources using various compilers
///     - Creating macros to optionally compile tests based on the availability 
///       of  compiler tools on the system being tested (runtime check)
///
/// We want to compile test binaries in the build script to avoid having
/// to recompile the same sources multiple times unnecessarily
/// 
/// Creating compile-time macros that rely on a runtime check
/// is only possible via procedural macros and a build script,
/// and a build script seems like the easier option


use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use ccargo::cc::{host_triple, which};


// Detect support of various compiler tools
    #[derive(Debug)]
pub struct Support {
    gcc: Option<PathBuf>,
    gxx: Option<PathBuf>,
    clang: Option<PathBuf>,
    clangxx: Option<PathBuf>,
    msvc: Option<PathBuf>,
}

impl Support {
    fn new() -> Self {
        Self{
            gcc: which("gcc"),
            gxx: which("g++"),
            clang: which("clang"),
            clangxx: which("clang++"),
            msvc: cc::windows_registry::find_tool(
                host_triple(), 
                "cl.exe"
            ).map(|v| v.path().to_path_buf()),
        }
    }
}

mod macros {
    use super::*;

    pub fn main(support: &Support, out_dir: &Path) {
        fn gen_macro(name: &str, path: &Option<PathBuf>) -> String {
            let body = if path.is_some() { "$($body)*" } else { "" };
            format!("#[macro_export]\nmacro_rules! {name} {{ ($($body:tt)*) => {{ {body} }} }}\n")
        }

        let mut src = String::new();
        src.push_str(&gen_macro("msvc", &support.msvc));
        src.push_str(&gen_macro("gcc", &support.gcc));
        src.push_str(&gen_macro("gxx", &support.gxx));
        src.push_str(&gen_macro("clang", &support.clang));
        src.push_str(&gen_macro("clangxx", &support.clangxx));
        
        fs::write(out_dir.join("tool_macros.rs"), src).unwrap();
    }
}


mod compile {
    use super::*;    
    use ccargo::cc::*;

    pub fn main(support: &Support, out_dir: &Path) {
        let tcs = Toolchains::new(support);
        
        for profile in [Profile::dev(), Profile::release()] {
            for tc in tcs.c() {
                // regular C executable
                build_exe(
                    "src/other/main.c",
                    Language::C,
                    profile.clone(),
                    tc,
                    out_dir
                );
                // C static library + executable
                build_lib(
                    "static",
                    BinType::Static,
                    profile.clone(),
                    tc,
                    out_dir
                );
                // C shared library + executable
                build_lib(
                    "shared",
                    BinType::Shared,
                    profile.clone(),
                    tc,
                    out_dir
                );
            }
        }
        
        for tc in tcs.c() {
            // Assembly executable
            build_asm(tc, out_dir);
        }

        for tc in tcs.cxx() {
            // regular C++ executable
            build_exe(
                "src/other/main.cpp",
                Language::Cxx,
                Profile::dev(),
                tc,
                out_dir
            );
        }

        // msvc-only
        if let Some(tc) = tcs.msvc.as_ref() {
            // C executable that requires <windows.h>
            Build::new("main_windows", BinType::Exe, tc.clone())
                .out_dir(out_dir)
                .force_lang(Language::C)
                .profile(Profile::dev())
                .file("src/other/windows.c")
                .compile()
                .unwrap();
        }
    }

    fn build_exe(
        path: &str,
        lang: Language,
        profile: Profile,
        toolchain: &Toolchain,
        out_dir: &Path,
    ) {
        let name = format!(
            "main_{}{}{}", 
            toolchain.tools_for(lang).unwrap().cc.name(),
            if profile.is_optimized() { "_rel" } else { "" },
            if lang.is_cxx() { "_cxx" } else { "" }
        );
        Build::new(&name, BinType::Exe, toolchain.clone())
            .out_dir(out_dir)
            .force_lang(lang)
            .profile(profile)
            .file(path)
            .compile()
            .unwrap();
    }

    fn build_asm(
        toolchain: &Toolchain,
        out_dir: &Path,
    ) {
        let tool = &toolchain.tools_for(Language::C).unwrap().cc;
        let asm_ext = if tool.family().is_msvc() {
            "asm"
        } else {
            "S"
        };
        Build::new(&format!("asm_{}", tool.name()), BinType::Exe, toolchain.clone())
            .out_dir(out_dir)
            .force_lang(Language::C)
            .profile(Profile::dev())
            .file(format!("src/asm/x86_64.{}", asm_ext))
            .file("src/asm/main.c")
            .compile()
            .unwrap();
    }

    fn build_lib(
        prefix: &str,
        bin_type: BinType,
        profile: Profile,
        toolchain: &Toolchain,
        out_dir: &Path,
    ) {
        let name = format!(
            "{}_{}{}", 
            prefix,
            toolchain.tools_for(Language::C).unwrap().cc.name(),            
            if profile.is_optimized() { "_rel" } else { "" },
        );

        // This is normally handled automatically but we want to compile
        // the same library `foo` with the same sources multiple times with
        // different tools, so we need to manually define these values
        // due to the unique names generated for each version of `foo`
        let mut options = Options::default();
        if bin_type.is_static() { 
            options.defines.insert("FOO_STATIC".into()); 
        } else {
            options.defines.insert("FOO_EXPORTS".into()); 
        }

        let r = Build::new(&format!("lib_{name}"), bin_type, toolchain.clone())
            .out_dir(out_dir)
            .force_lang(Language::C)
            .options(options)
            .profile(profile.clone())
            .file("src/lib/foo.c")
            .compile()
            .unwrap();

        let mut options = Options::default();
        if bin_type.is_static() { 
            options.defines.insert("FOO_STATIC".into()); 
        }
        Build::new(&name, BinType::Exe, toolchain.clone())
            .out_dir(out_dir)
            .options(options)
            .profile(profile)
            .file("src/lib/main.c")
            .library(&r.path)
            .compile()
            .unwrap();
    }

    #[derive(Debug)]
    struct Toolchains {
        gcc: Option<Toolchain>,
        gxx: Option<Toolchain>,
        clang: Option<Toolchain>,
        clangxx: Option<Toolchain>,
        msvc: Option<Toolchain>,
    }
    
    impl Toolchains {
        fn new(support: &Support) -> Self {
            Self{
                gcc: support.gcc.as_ref().map(|p| Toolchain::new_host(p.clone(), None).unwrap()),
                gxx: support.gxx.as_ref().map(|p| Toolchain::new_host(None, p.clone()).unwrap()),
                clang: support.clang.as_ref().map(|p| Toolchain::new_host(p.clone(), None).unwrap()),
                clangxx: support.clangxx.as_ref().map(|p| Toolchain::new_host(None, p.clone()).unwrap()),
                msvc: support.msvc.as_ref().map(|_| Toolchain::default().unwrap()),
            }
        }

        fn c(&self) -> impl Iterator<Item=&Toolchain> {
            [self.gcc.as_ref(), self.clang.as_ref(), self.msvc.as_ref()]
                .into_iter()
                .filter_map(|v| v)
        }

        fn cxx(&self) -> impl Iterator<Item=&Toolchain> {
            [self.gxx.as_ref(), self.clangxx.as_ref(), self.msvc.as_ref()]
                .into_iter()
                .filter_map(|v| v)
        }
    }

}

fn main() {
    let support = Support::new();
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    
    println!("cargo:rerun-if-changed=build.rs");    
    macros::main(&support, &out_dir);

    
    println!("cargo:rerun-if-changed=src/other/main.c");
    println!("cargo:rerun-if-changed=src/other/main.cpp");
    println!("cargo:rerun-if-changed=src/other/windows.c");

    println!("cargo:rerun-if-changed=src/lib/foo-export.h");
    println!("cargo:rerun-if-changed=src/lib/foo.h");
    println!("cargo:rerun-if-changed=src/lib/foo.c");
    println!("cargo:rerun-if-changed=src/lib/main.c");
    
    println!("cargo:rerun-if-changed=src/asm/aarch64.asm");
    println!("cargo:rerun-if-changed=src/asm/aarch64.S");    
    println!("cargo:rerun-if-changed=src/asm/i686.asm");
    println!("cargo:rerun-if-changed=src/asm/i686.S");    
    println!("cargo:rerun-if-changed=src/asm/x86_64.asm");
    println!("cargo:rerun-if-changed=src/asm/x86_64.S");
    println!("cargo:rerun-if-changed=src/asm/armv7.S");
    println!("cargo:rerun-if-changed=src/asm/riscv64gc.S");

    compile::main(&support, &out_dir);
}
