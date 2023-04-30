use ccargo::cc::{Message, ToolKind, ToolFamily};

macro_rules! test_output {
    ($path:expr) => {
        let record = std::env::var_os("RECORD").is_some();
        let out_dir = std::path::PathBuf::from("tests/outputs");
        let in_path = out_dir.join(format!("{}.in", $path));
        let out_path = out_dir.join(format!("{}.out", $path));
        let input = std::fs::read(in_path).unwrap();

        let colors = $path.contains("colors");

        let family = if $path.contains("msvc") {
            ToolFamily::Msvc
        } else if $path.contains("clang") {
            ToolFamily::Clang
        } else {
            ToolFamily::Gnu
        };

        let kind = if $path.contains("compile") {
            ToolKind::Compiler
        } else {
            ToolKind::Linker
        };

        let mut output = Vec::new();
        for msg in Message::iter(
            input.as_slice(),
            kind,
            family,
            $path.contains("windows"),
            colors,
        ) {
            msg.print(&mut output, colors).unwrap();
        }

        if record {
            std::fs::write(out_path, output).unwrap();
        } else {        
            let expected = std::fs::read(out_path).unwrap();
            if output != expected {
                assert_eq!(
                    String::from_utf8(input).unwrap(), 
                    String::from_utf8(expected).unwrap()
                );
            }
        }
    };
}

mod gcc {
    use super::*;
    
    mod compile {
        use super::*;
        
        #[test]
        fn error_plain_win32() { test_output!("windows/gcc/plain/compile_error.stderr"); }

        #[test]
        fn error_plain_unix() { test_output!("linux/gcc/plain/compile_error.stderr"); }

        #[test]
        fn warning_plain_win32() { test_output!("windows/gcc/plain/compile_warning.stderr"); }

        #[test]
        fn warning_plain_unix() { test_output!("linux/gcc/plain/compile_warning.stderr"); }

        #[test]
        fn error_colors_unix() { test_output!("linux/gcc/colors/compile_error.stderr"); }

        #[test]
        fn warning_colors_unix() { test_output!("linux/gcc/colors/compile_warning.stderr"); }
    }

    mod link {
        use super::*;

        #[test]
        fn error_plain_win32() { test_output!("windows/gcc/plain/link_error.stderr"); }
        
        #[test]
        fn error_plain_unix() { test_output!("linux/gcc/plain/link_error.stderr"); }

        #[test]
        fn warning_plain_unix() { test_output!("linux/gcc/plain/link_warning.stderr"); }
        
        #[test]
        fn error_colors_unix() { test_output!("linux/gcc/colors/link_error.stderr"); }

        #[test]
        fn warning_colors_unix() { test_output!("linux/gcc/colors/link_warning.stderr"); }
    }
}

mod clang {
    use super::*;
    
    mod compile {
        use super::*;
        
        #[test]
        fn error_plain_win32() { test_output!("windows/clang/plain/compile_error.stderr"); }

        #[test]
        fn error_plain_unix() { test_output!("linux/clang/plain/compile_error.stderr"); }

        #[test]
        fn warning_plain_win32() { test_output!("windows/clang/plain/compile_warning.stderr"); }

        #[test]
        fn warning_plain_unix() { test_output!("linux/clang/plain/compile_warning.stderr"); }

        #[test]
        fn error_colors_unix() { test_output!("linux/clang/colors/compile_error.stderr"); }

        #[test]
        fn warning_colors_unix() { test_output!("linux/clang/colors/compile_warning.stderr"); }
    }   

    mod link {
        use super::*;

        #[test]
        fn error_plain_win32() { test_output!("windows/clang/plain/link_error.stderr"); }
        
        #[test]
        fn error_plain_unix() { test_output!("linux/clang/plain/link_error.stderr"); }

        #[test]
        fn warning_plain_unix() { test_output!("linux/clang/plain/link_warning.stderr"); }
        
        #[test]
        fn error_colors_unix() { test_output!("linux/clang/colors/link_error.stderr"); }

        #[test]
        fn warning_colors_unix() { test_output!("linux/clang/colors/link_warning.stderr"); }
    }
}

mod msvc {
    use super::*;
    
    mod compile {
        use super::*;
        
        #[test]
        fn error() { test_output!("windows/msvc/plain/compile_error.stdout"); }

        #[test]
        fn warning() { test_output!("windows/msvc/plain/compile_warning.stdout"); }

        #[test]
        fn warning_error() { test_output!("windows/msvc/plain/compile_warning_error.stdout"); }
    }   

    mod link {
        use super::*;

        #[test]
        fn error() { test_output!("windows/msvc/plain/link_error.stdout"); }

        #[test]
        fn warning() { test_output!("windows/msvc/plain/link_warning.stdout"); }

        #[test]
        fn warning_error() { test_output!("windows/msvc/plain/link_warning_error.stdout"); }
    }
}
