use ccargo::core::*;
use ccargo::cc::{Profile, Toolchain};
use ccargo::toml::read_package;
use ccargo::utils::IResult;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// SDK
//      name
//      includes
//      libraries
//      tools
//
//      find
//      install


/// Load all
pub fn load_packages(
    path: &Path,
    config: &Config,
) -> IResult<(Package, PackageMap)> {
    // TODO: Verify all shared targets have a unique name
    // TODO: Verify there are no runtime lib conflicts
    // TODO: Verify that target dependencies are only libraries
    // TODO: Verify all files exist or are step outputs
    // TODO: Verify every bin target that is depended on by other units has no dylib dependencies itself
    // TODO: Recursively load CCargo.toml files inside package root dir
    let mut packages = Vec::new();
    load_packages_recursive(path, config, &mut packages, &mut HashSet::new())?;
    Ok((
        // first package is the root
        packages.first().unwrap().clone(), 
        PackageMap::from_packages(&packages)
    ))
}

fn load_packages_recursive(
    path: &Path,
    config: &Config,
    packages: &mut Vec<Package>,
    seen: &mut HashSet<PathBuf>,
) -> IResult<()> {
    if !seen.insert(path.to_path_buf()) {
        return Ok(());
    }
    let pkg = read_package(path, config)?;
    packages.push(pkg.clone());
    for dep in pkg.dependencies.iter() {
        load_packages_recursive(&dep.source_id.manifest_path(), config, packages, seen)?;
    }
    Ok(())
}


use std::time::{Instant, Duration};

struct Timer {
    prev: Instant
}

impl Timer {
    pub fn new() -> Self {
        Self{prev: Instant::now()}
    }

    pub fn elapsed(&mut self) -> Duration {
        let now = Instant::now();
        let t = now - self.prev;
        self.prev = now;
        t
    }

    pub fn print_elapsed(&mut self, name: &str) {
        let t = self.elapsed();        
        println!("{name:16} {:.3}s", t.as_secs_f64());
    }
}


fn main() {
    let root = std::env::current_dir().unwrap().join("target/tmp");
    let path = root.join("CCargo.toml");
    
    let mut timer = Timer::new();
    let config = Config::default().unwrap();
    let profile = Profile::dev();
    let layout = Layout::new(root, &profile, None);    
    let (main_package, packages) = load_packages(&path, &config).unwrap();
    let main_target = main_package.targets.last().unwrap();
    let toolchain = Toolchain::default().unwrap();
    timer.print_elapsed("Load");

    for warning in main_package.warnings.iter() {
        drop(config.shell().warn(warning));
    }
    
    let cx = Context::new(
        &config,
        &layout, 
        &toolchain,
        &profile,
        &packages, 
        &main_package,
        &["foo".to_string()],
    );
    cx.compile().unwrap();
    timer.print_elapsed("Build");
    // println!("{:16} {:.3}s", "Compile", config.creation_time().elapsed().as_secs_f64());
    
    if cx.units.is_empty() {
        return;
    }
    cx.run(&main_target, true).unwrap();
    timer.print_elapsed("Run");
}

// 1. Parse args/config
//      - choose package/target
//      - choose platform

// 2. Detect toolchain
//      - detect host compiler/features
//      - detect target compiler/features

// 3. Load packages
//      - load dependencies recursively
//      - load nested `CCargo.toml`s recursively
//      - warn unused keys
//      - convert toml to real
//          - absolute paths
//          - inherit from project
//          - platform-specific properties
//          - validate properties

// 4. Build graph
//      - create package map
//      - calculate inputs/outputs
//      - calculate unit graph

// 5. Execute graph
//      - parallel execution
//      - check fingerprint, replay output
//      - otherwise execute and update fingerprints

// 6. Run binaries
//      - exec_replace
//      - test runner
//          - collect tests
//          - run tests
//          - print test results
//      - bench runner
//          - collect tests
//          - run tests as benchmark
//          - print test results
