mod support;
use support::Test;
use ccargo::cc::*;

#[test]
fn gnu_debug() {
    let test = Test::gnu();
    
    let r = test.cc(BinType::Exe)
        .file("foo.c")
        .compile()
        .unwrap();

    test.cmd(0)
        .must_have("-c")
        .must_have("foo.c")
        .must_have("-O0")
        .must_have("-Wall")
        .must_have("-std=c11")
        .must_have("-m64")
        .must_have("-gdwarf-4")
        .must_have("-ffunction-sections")
        .must_have("-fdata-sections")
        .must_have("-fvisibility=hidden")
        .must_have("-MMD")
        .must_have("-MF")
        .must_have(&r.objs[0].0.dst)
        .must_have(r.objs[0].0.dep())
        .must_not_have("-flto");

    test.cmd(1)
        .must_have("-static")
        .must_have("-Wl,-rpath,$ORIGIN")
        .must_have(&r.objs[0].0.dst)
        .must_not_have("-flto")
        .must_have("-o")
        .must_have(&r.path);
}


#[test]
fn gnu_release() {
    let test = Test::gnu();
    
    let _ = test.cc(BinType::Exe)
        .file("foo.c")
        .profile(Profile::release())
        .compile()
        .unwrap();

    test.cmd(0)
        .must_have("-c")
        .must_have("foo.c")
        .must_have("-O3")
        .must_have("-flto")
        .must_not_have("-gdwarf-4");

    test.cmd(1)
        .must_have("-flto");
}

#[test]
fn msvc_debug() {
    let test = Test::msvc();
    
    let r = test.cc(BinType::Exe)
        .file("foo.c")
        .compile()
        .unwrap();

    test.cmd(0)
        .must_have("-nologo")
        .must_have("-c")
        .must_have("foo.c")
        .must_have("-TC")
        .must_have("-Od")
        .must_have("-Ob0")
        .must_have("-Zi")
        .must_have("-RTC1")
        .must_have("-Gd")
        .must_have("-W3")
        .must_have("-MTd")
        .must_have("-std:c11")
        .must_have("-Zc:preprocessor")
        .must_have("-Zc:inline")
        .must_have("-Zc:forScope")
        .must_have("-Zc:wchar_t")
        .must_have("-fp:precise")
        .must_have("-Brepro")
        .must_have("-DWIN32")
        .must_have("-D_WINDOWS")
        .must_have("-D_MBCS")
        .must_have(format!("-Fo{}", r.objs[0].0.dst.display()))
        .must_have(format!("-Fd{}", r.path.with_extension("pdb").display()));

    test.cmd(1)
        .must_have("-nologo")
        .must_have("-machine:x64")
        .must_have("-DYNAMICBASE")
        .must_have("-NXCOMPAT")
        .must_have("-INCREMENTAL")
        .must_have("-DEBUG")
        .must_have("kernel32.lib")
        .must_have(&r.objs[0].0.dst)
        .must_have(format!("-OUT:{}", r.path.display()))
        .must_have(format!("-IMPLIB:{}", r.path.with_extension("lib").display()))
        .must_have(format!("-PDB:{}", r.path.with_extension("pdb").display()))
        .must_have(format!("-ILK:{}", r.path.with_extension("ilk").display()));
}


#[test]
fn msvc_release() {
    let test = Test::msvc();
    
    let r = test.cc(BinType::Exe)
        .file("foo.c")
        .profile(Profile::release())
        .compile()
        .unwrap();

    test.cmd(0)
        .must_have("-O2")
        .must_have("-Ob2")
        .must_not_have("-Zi")
        .must_not_have("-RTC1");

    test.cmd(1)
        .must_have("-INCREMENTAL:NO")
        .must_not_have("-DEBUG")
        .must_not_have(format!("-PDB:{}", r.path.with_extension("pdb").display()))
        .must_not_have(format!("-ILK:{}", r.path.with_extension("ilk").display()));
}
