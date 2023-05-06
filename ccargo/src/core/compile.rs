use crate::core::*;
use crate::cc::{Build, Profile, Toolchain, Language, Artifact, Output as CCOutput};
use crate::utils::{Graph, MsgQueue, IResult, CommandExt, lev_distance};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// TODO: Target/Toolchain overhaul
// Rust did all the work for us already, we just need to use it

pub struct Context<'a> {
    pub config: &'a Config,
    pub layout: &'a Layout,
    pub toolchain: &'a Toolchain,
    pub profile: &'a Profile,
    pub units: UnitMap,
    pub target_deps: TargetDepsMap,
    pub target_io: TargetIOMap,
    pub unit_graph: Graph<Unit>,
    pub fingerprints: Mutex<HashMap<Unit, Arc<Fingerprint>>>,
}

/// Inputs/Outputs for a given `Target`
pub type TargetIOMap = HashMap<Target, TargetIO>;

/// TargetDepsMap
pub type TargetDepsMap = HashMap<Target, TargetDeps>;

#[derive(Debug)]
pub struct TargetIO {
    pub output: PathBuf,
    pub artifacts: Vec<Artifact>,
    pub deps: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct TargetDeps {
    pub libs: Vec<PathBuf>,
    pub includes: BTreeSet<PathBuf>,
    pub defines: BTreeMap<String, Option<String>>,
}


impl<'a> Context<'a> {
    pub fn new(
        config: &'a Config,
        layout: &'a Layout,
        toolchain: &'a Toolchain,
        profile: &'a Profile,
        packages: &PackageMap,
        main_package: &Package,
        selected: &[String],
    ) -> Self {
        let target_triple = toolchain.target();

        let mut cx = Self{
            config,
            layout,
            toolchain,
            profile,
            units: UnitMap::default(),
            target_deps: TargetDepsMap::new(),
            unit_graph: Graph::new(),
            target_io: HashMap::new(),
            fingerprints: Mutex::default(),
        };

        // Convert selected names into targets
        let selected = selected_targets(config, main_package, selected);
        if selected.is_empty() {
            return cx;
        }

        // Build unit map
        cx.units = UnitMap::from_package_map(packages, layout, target_triple);

        // Build unit graph
        cx.unit_graph = cx.units.build_graph(&selected);

        // Collect dependency information for targets
        for unit in cx.unit_graph.nodes() {
            let target = if let Unit::Target(target) = unit {
                target
            } else {
                continue
            };
            cx.target_deps.insert(target.clone(), TargetDeps::new(&cx, target));
        }

        // Calculate target inputs/outputs
        for unit in cx.unit_graph.nodes() {
            let target = if let Unit::Target(target) = unit {
                target
            } else {
                continue
            };
            let io = TargetIO::from_target(
                target,
                layout,
                &toolchain,
                &profile,
                &cx.target_deps
            );
            cx.target_io.insert(target.clone(), io);
        }
        
        cx
    }
    
    // TODO: Check all units

    pub fn compile(&self) -> IResult<()> {
        // Execute unit graph in parallel
        let outputs = Outputs::default();
        let n_units = self.units.len();
        let stdout = MsgQueue::new(n_units, std::io::stdout());
        let stderr = MsgQueue::new(n_units, std::io::stderr());
        
        // TODO: Execute units in parallel
        for stage in self.unit_graph.parallel_stages() {
            for unit in stage {
                compile_unit(
                    self, 
                    unit, 
                    &outputs,
                    &stdout, 
                    &stderr
                )?;
            }
        }
        
        // Copy outputs to target directory
        outputs.copy_to(&self.layout.target())
    }
    
    pub fn run(&self, target: &Target, is_main: bool) -> IResult<()> {
        if let TargetKind::Static | TargetKind::Shared = target.kind {
            anyhow::bail!("Cannot run library target `{}`", target.full_name())
        }

        let exe = if is_main {
            self.layout.target().join(target.output_name(self.toolchain.target()))
        } else {
            target.output_path(self.layout, self.toolchain.target())
        };

        std::process::Command::new(&exe).exec_replace()?;

        Ok(())
    }
}


fn compile_unit(
    cx: &Context,
    unit: &Unit,
    outputs: &Outputs,
    stdout: &MsgQueue<std::io::Stdout>,
    stderr: &MsgQueue<std::io::Stderr>,
) -> IResult<()> {
    let fingerprint_path = unit.fingerprint_path(cx.layout);
    let (fingerprint, state) = fingerprint::prepare(
        cx,
        unit,
        &fingerprint_path,
    )?;

    if state.is_fresh() {
        return Ok(());
    }

    use crate::utils::{ColorString, WriteColorExt, Color};
    let stdout = stdout.writer();
    match unit {
        Unit::Target(target) => {            
            drop(stdout.push({
                let mut msg = ColorString::new();
                drop(msg.write_status_justified(
                    &"Compiling", 
                    Some(&target.full_name()), 
                    Color::Green,
                ));
                msg
            }.as_bytes()));
            
            let output = target.compile(
                cx,
                &state,
                stdout,
                stderr.writer(),
            )?;                
            outputs.add(cx, target, &output);
        }
        Unit::Step(step) => {
            drop(stdout.push({
                let mut msg = ColorString::new();
                drop(msg.write_status_justified(
                    &"Running", 
                    Some(&step.full_name()), 
                    Color::Green,
                ));
                msg
            }.as_bytes()));

            let status = step.run(
                cx,
                stdout,
                stderr.writer(),
            )?;
            if !status.success() {
                match status.code() {
                    Some(code) => anyhow::bail!("Step `{}` exited with error code {}", step.full_name(), code),
                    None => anyhow::bail!("Step `{}` was terminated by signal", step.full_name()),
                }
            }
        }
    }

    fingerprint::write_to_disk(
        &fingerprint,
        &fingerprint_path,
    )?;

    Ok(())
}


fn selected_targets(
    config: &Config,
    main_package: &Package,
    selected: &[String],
) -> Vec<Target> {
    let mut targets = Vec::new();
    
    for name in selected {
        let target = main_package.targets
            .iter()
            .find(|t| t.name.as_str() == name.as_str());
        
        match target {
            Some(target) => {
                targets.push(target.clone());
            }
            None => {
                drop(config.shell().warn(&format!(
                    "Package `{}` does not have a target with name `{}`{}", 
                    main_package.name(), 
                    name,
                    lev_distance::closest_msg(
                        name, 
                        main_package.targets.iter(),
                        |t| t.name.as_str()
                    )
                )));
            }
        }
    }

    targets
}


impl TargetIO {
    fn from_target(
        target: &Target,
        layout: &Layout,
        toolchain: &Toolchain,
        profile: &Profile,
        target_deps: &TargetDepsMap, 
    ) -> Self {
        let bin_type = target.kind.into();

        let lang = if target.sources.iter().any(|v| Language::detect(&v).is_cxx()) {
            Language::Cxx
        } else {
            Language::C
        };

        let output = layout.output_dir(&target.package).join(Build::output_name(
            &target.name, 
            bin_type, 
            &toolchain
        ));

        let artifacts = Build::output_artifacts(
            bin_type, 
            lang, 
            &toolchain, 
            &profile
        );

        Self {
            output,
            artifacts,
            deps: target_deps[target].libs.clone(),
        }
    }
}

impl TargetDeps {
    fn new(
        cx: &Context,
        target: &Target,
    ) -> Self {
        let mut deps = TargetDeps::default();        
        deps.collect(cx, target);
        // Add defines last so we can overwrite dep defines
        for include in target.includes.iter() {
            deps.includes.insert(include.to_path_buf());
        }
        for define in target.defines.iter() {
            let (key, value) = std::ops::Deref::deref(define).clone();
            deps.defines.insert(key, value);
        }
        deps
    }


    fn collect(
        &mut self,
        cx: &Context,
        target: &Target,
    ) {
        let target_triple = cx.toolchain.target();

        for dep_name in target.depends.iter() {
            let dep = cx.units.get(dep_name, &target.package);

            let target = if let Unit::Target(target) = dep {
                target
            } else {
                continue;
            };

            self.libs.push(target.output_path(cx.layout, target_triple));
            
            for include in target.includes.iter() {
                if include.is_public() {
                    self.includes.insert(include.to_path_buf());
                }
            }

            for define in target.defines.iter() {
                if define.is_public() {
                    let (key, value) = std::ops::Deref::deref(define).clone();
                    self.defines.insert(key, value);
                }
            }

            self.collect(cx, target);
        }
    }

}


/// List of (output_path, target_path) pairs for each compilation output
/// This is used to copy required compilation outputs into the target directory.
#[derive(Default)]
struct Outputs {
    outputs: Mutex<Vec<Output>>,
}

struct Output {
    src: PathBuf,
    dst: Option<PathBuf>,
    updated: bool,
}

impl Outputs {
    fn add(
        &self, 
        cx: &Context, 
        target: &Target, 
        output: &CCOutput,
    ) {
        if target.kind == TargetKind::Static {
            return;
        }

        let updated = output.did_link;
        let runtime_dst = target.runtime_path(cx.layout, cx.toolchain.target());

        // output goes into runtime directory first, otherwise target directory
        self.outputs.lock().unwrap().push(Output{
            updated,
            src: output.path.clone(), 
            dst: runtime_dst.clone(), 
        });

        // debug info goes next to the target output file
        for artifact in output.extra.iter() {
            let ext = artifact.extension().and_then(|x| x.to_str()).unwrap();
            if ext == Artifact::Pdb.ext() || ext == Artifact::Dsym.ext() {
                self.outputs.lock().unwrap().push(Output{ 
                    dst: runtime_dst.as_ref().map(|v| v.with_extension(ext)), 
                    src: artifact.clone(),
                    updated,
                });
            }
        }
    }

    fn copy_to(&self, dst: &Path) -> IResult<()> {
        for output in self.outputs.lock().unwrap().iter() {
            if output.updated {
                std::fs::copy(
                    &output.src, 
                    output.dst.clone().unwrap_or_else(||
                        dst.join(output.src.file_name().unwrap())),
                )?;
            }
        }
        Ok(())
    }
}
