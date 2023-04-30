mod support;

// TODO: Test check and expand
// TODO: Test stderr cache

fn assert_code(name: &str, code: i32) {
    let path = support::exes_root()
        .join(name)
        .with_extension(std::env::consts::EXE_EXTENSION);
    assert_eq!(Some(code), support::exe_status(&path));
}


fn assert_dep_info(name: &str, contains: &[&str]) {
    let path = support::exes_root().join(format!("lib_{name}.dir"));
    for d in std::fs::read_dir(&path).unwrap() {
        let d = d.unwrap().path();
        if d.extension().unwrap() == "o.d" {
            let data = std::fs::read_to_string(d).unwrap();
            for item in contains {
                assert!(data.contains(item));
            }
            return;
        }
    }
}


gcc!(mod gcc {
    use super::*;

    #[test]
    fn exe() { assert_code("main_gcc", 42); }

    #[test]
    fn static_() { assert_code("static_gcc", 42); }

    #[test]
    fn shared() { assert_code("shared_gcc", 42); }

    #[test]
    fn exe_rel() { assert_code("main_gcc_rel", 42); }
    
    #[test]
    fn static_rel() { assert_code("static_gcc_rel", 42); }

    #[test]
    fn shared_rel() { assert_code("shared_gcc_rel", 42); }
    
    #[test]
    fn asm() { assert_code("asm_gcc", 42); }

    #[test]
    fn dep_info() { 
        assert_dep_info(
            "static_gcc", 
            &[
                "src/lib/foo.h",
                "src/lib/foo-export.h",
            ],
        )
    }
});

clang!(mod clang {
    use super::*;

    #[test]
    fn exe() { assert_code("main_clang", 42); }

    #[test]
    fn static_() { assert_code("static_clang", 42); }

    #[test]
    fn shared() { assert_code("shared_clang", 42); }

    #[test]
    fn exe_rel() { assert_code("main_clang_rel", 42); }
    
    #[test]
    fn static_rel() { assert_code("static_clang_rel", 42); }

    #[test]
    fn shared_rel() { assert_code("shared_clang_rel", 42); }
    
    #[test]
    fn asm() { assert_code("asm_clang", 42); }

    #[test]
    fn dep_info() { 
        assert_dep_info(
            "static_clang", 
            &[
                "src/lib/foo.h",
                "src/lib/foo-export.h",
            ],
        )
    }
});

msvc!(mod msvc {
    use super::*;

    #[test]
    fn exe() { assert_code("main_cl", 42); }

    #[test]
    fn static_() { assert_code("static_cl", 42); }

    #[test]
    fn shared() { assert_code("shared_cl", 42); }

    #[test]
    fn exe_rel() { assert_code("main_cl_rel", 42); }
    
    #[test]
    fn static_rel() { assert_code("static_cl_rel", 42); }

    #[test]
    fn shared_rel() { assert_code("shared_cl_rel", 42); }
    
    #[test]
    fn asm() { assert_code("asm_cl", 42); }

    #[test]
    fn exe_windows() { assert_code("main_windows", 2560); }

    #[test]
    fn dep_info() { 
        assert_dep_info(
            "static_cl", 
            &[
                "src/lib/foo.h",
                "src/lib/foo-export.h",
            ],
        )
    }
});

gxx!(mod gxx { 
    use super::*;

    #[test]
    fn exe() { assert_code("main_g++_cxx", 42); }
});

clangxx!(mod clangxx { 
    use super::*;

    #[test]
    fn exe() { assert_code("main_clang++_cxx", 42); }
});
