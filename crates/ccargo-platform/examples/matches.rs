//! This example demonstrates how to filter a Platform based on the current
//! host target.

use ccargo_platform::{Platform, RustcTarget};
use std::str::FromStr;

static EXAMPLES: &[&str] = &[
    "cfg(windows)",
    "cfg(unix)",
    "cfg(target_os=\"macos\")",
    "cfg(target_os=\"linux\")",
    "cfg(any(target_arch=\"x86\", target_arch=\"x86_64\"))",
];

fn main() {
    let t = RustcTarget::detect_sync().unwrap();
    println!("host target={} cfgs:", t.target());
    for cfg in t.cfgs() {
        println!("  {}", cfg);
    }
    let mut examples: Vec<&str> = EXAMPLES.iter().copied().collect();
    examples.push(t.target());
    for example in examples {
        let p = Platform::from_str(example).unwrap();
        println!("{:?} matches: {:?}", example, p.matches(t.target(), t.cfgs()));
    }
}
