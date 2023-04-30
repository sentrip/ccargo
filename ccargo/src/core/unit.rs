use crate::core::*;
use crate::utils::Graph;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};


/// Graph of units that depend on eachother
pub type UnitGraph = Graph<Unit>;


/// Unit of compilation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Unit {
    Target(Target),
    Step(Step),
}

impl Unit {
    pub fn package(&self) -> PackageId {
        match self {
            Self::Target(v) => v.package,
            Self::Step(v) => v.package,
        }
    }
    
    pub fn full_name(&self) -> TargetName {
        match self {
            Self::Target(v) => v.full_name(),
            Self::Step(v) => v.full_name(),
        }
    }
    
    pub fn fingerprint_path(&self, layout: &Layout) -> PathBuf {
        match self {
            Self::Target(v) => v.fingerprint_path(layout),
            Self::Step(v) => v.fingerprint_path(layout),
        }
    }

    pub fn for_each_dep<F: FnMut(&TargetName) -> ()>(&self, mut func: F) {
        match self {
            Self::Target(v) => {
                for dep in v.depends.iter() {
                    func(dep);
                }
            }
            Self::Step(v) => {
                for dep in v.depends.iter() {
                    func(dep);
                }
            }
        }
    }
    
    pub fn as_target(&self) -> Option<&Target> {
        if let Self::Target(target) = self {
            Some(target)
        } else {
            None
        }
    }
    
    pub fn as_step(&self) -> Option<&Step> {
        if let Self::Step(step) = self {
            Some(step)
        } else {
            None
        }
    }
}

/// UnitMap - in package with id A, which target does target name B refer to?
#[derive(Default)]
pub struct UnitMap {
    // Set of all units
    units: HashSet<Unit>,
    // Ambiguous units that require a PackageID to be resolved
    unit_map: HashMap<TargetName, HashMap<PackageId, Unit>>,
    // Units with an output path can be mapped by path
    unit_outputs: HashMap<PathBuf, Unit>,
    // Steps with an output path can be mapped by path
    step_outputs: HashMap<PathBuf, Step>,
}

impl UnitMap {
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub fn len(&self) -> usize {
        self.unit_map.values().map(|x| x.len()).sum()
    }
    
    pub fn get(&self, name: &TargetName, src: &PackageId) -> &Unit {
        self.unit_map
            .get(name)
            .and_then(|m| m.get(src))
            .unwrap()
    }

    pub fn with_output(&self, path: &Path) -> Option<&Unit> {
        self.unit_outputs.get(path)
    }

    pub fn step_with_output(&self, path: &Path) -> Option<&Step> {
        self.step_outputs.get(path)
    }

    pub fn named(&self, name: &TargetName) -> Option<&Unit> {
        self.unit_map
            .get(name)
            .and_then(|m| {
                if m.len() == 1 {
                    Some(m.values().next().unwrap())
                } else {
                    None
                }
            })
    }

    pub fn from_package_map(
        packages: &PackageMap,
        layout: &Layout,
        target_triple: &str,
    ) -> Self {
        let mut map = Self::default();
        for pkg in packages.iter() {

            for target in pkg.targets.iter() {
                let unit = Unit::Target(target.clone());

                map.units.insert(unit.clone());

                map.unit_outputs
                    .insert(target.output_path(layout, target_triple), unit.clone());

                map.unit_map
                    .entry(target.full_name())
                    .or_default()
                    .insert(pkg.id, unit.clone());

                for dep in target.depends.iter() {
                    map.add_dep(packages, pkg, dep);
                }
            }

            for step in pkg.steps.iter() {
                let unit = Unit::Step(step.clone());

                map.units.insert(unit.clone());

                for output in step.outputs.iter() {
                    map.step_outputs
                        .insert(output.clone(), step.clone());
                }

                map.unit_map
                    .entry(step.full_name())
                    .or_default()
                    .insert(pkg.id, unit.clone());

                for dep in step.depends.iter() {
                    map.add_dep(packages, pkg, dep);
                }
            }
        }

        map
    }
    
    pub fn build_graph(&self, selected: &[Target]) -> UnitGraph {
        let mut g = UnitGraph::new();
        
        for target in selected {
            self.build_graph_recursive(&Unit::Target(target.clone()), &mut g);
        }
        
        for unit in self.units.iter() {
            match unit {
                Unit::Target(target) => {
                    for source in target.sources.iter() {
                        if let Some(dep) = self.step_outputs.get(source) {
                            g.link(unit.clone(), Unit::Step(dep.clone()));
                        }
                    }
                }

                Unit::Step(step) => {
                    if let Some(dep_name) = step.target() {
                        g.link(unit.clone(), self.get(dep_name, &step.package).clone());
                    }
    
                    for input in step.inputs.iter() {
                        if let Some(dep) = self.step_outputs.get(input) {
                            g.link(unit.clone(), Unit::Step(dep.clone()));
                        }
                    }
                }
            }
        }
        
        g
    }
    
    fn build_graph_recursive(
        &self,
        unit: &Unit, 
        g: &mut UnitGraph,
    ) {
        g.add(unit.clone());
        unit.for_each_dep(|dep| {
            let dep_unit = self.get(dep, &unit.package());
            g.link(unit.clone(), dep_unit.clone());
            self.build_graph_recursive(dep_unit, g);
        });
    }
    
    fn add_dep(
        &mut self, 
        packages: &PackageMap,
        pkg: &Package,
        dep: &TargetName,
    ) {
        let dep_pkg = packages.named(&dep.package(), &pkg.id);
                    
        let dep_tgt_name = dep.target();
        let dep_tgt = dep_pkg.targets
            .iter()
            .find(|x| x.name == dep_tgt_name);

        if let Some(dep_tgt) = dep_tgt {
            self.unit_map
                .entry(dep.clone())
                .or_default()
                .insert(pkg.id, Unit::Target(dep_tgt.clone()));
        } else {
            let dep_step = dep_pkg.steps
                .iter()
                .find(|x| x.name == dep_tgt_name);
            
            if let Some(dep_step) = dep_step {
                self.unit_map
                    .entry(dep.clone())
                    .or_default()
                    .insert(pkg.id, Unit::Step(dep_step.clone()));
            } else {
                panic!("Cannot find dependency `{}`", dep);
            }
        }
    }
}
