
mod support;

use std::path::PathBuf;
use ccargo::cc::{Toolchain, ToolFamily, Language, Error};

fn new_c(tool: &str) -> Result<Toolchain, Error> {
    Toolchain::new_host(Some(PathBuf::from(tool)), None)
}

fn new_cxx(tool: &str) -> Result<Toolchain, Error> {
    Toolchain::new_host(None, Some(PathBuf::from(tool)))
}

macro_rules! assert_toolchain_ok {
    ($tc:expr, $family:expr, $lang:expr) => {
        let tc = $tc;
        assert!(tc.is_ok());
        let tc = tc.unwrap();
        assert_eq!($family, tc.tools_for($lang).unwrap().cc.family());
        assert_eq!($family, tc.tools_for($lang).unwrap().ld.family());
        
    };
}

gcc!(
    #[test]
    fn gcc() {
        assert_toolchain_ok!(
            new_c("gcc"), 
            ToolFamily::Gnu,
            Language::C
        );
    }
);


clang!(
    #[test]
    fn clang() {
        assert_toolchain_ok!(
            new_c("clang"), 
            ToolFamily::Clang,
            Language::C
        );
    }
);


msvc!(
    #[test]
    fn msvc() {
        assert_toolchain_ok!(
            Toolchain::default(), 
            ToolFamily::Msvc,
            Language::C
        );
    }
);


gxx!(
    #[test]
    fn gxx() {
        assert_toolchain_ok!(
            new_cxx("g++"), 
            ToolFamily::Gnu,
            Language::Cxx
        );
    }
);


clangxx!(
    #[test]
    fn clangxx() {
        assert_toolchain_ok!(
            new_cxx("clang++"), 
            ToolFamily::Clang,
            Language::Cxx
        );
    }
);
