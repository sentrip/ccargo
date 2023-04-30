use crate::core::{TargetName, PackageId, Layout, Context, Fingerprint};
use crate::utils::{IResult, InternedString, MsgWriter, paths};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::path::{Path, PathBuf};
use std::sync::Arc;


#[derive(Debug, Hash)]
pub enum Program {
    Binary(PathBuf),
    Target(TargetName),
    Script{tool: PathBuf, script: PathBuf},
}


#[derive(Clone)]
pub struct Step(Arc<StepInner>);

#[derive(Debug)]
pub struct StepInner {
     pub name: InternedString,
     pub package: PackageId,
     pub inputs: Vec<PathBuf>,
     pub outputs: Vec<PathBuf>,
     pub depends: Vec<TargetName>,
     pub program: Program,
     pub args: Vec<String>,
}


impl Step {
    pub fn new(inner: StepInner) -> Self {
        Self(Arc::new(inner))
    }
    
    pub fn full_name(&self) -> TargetName {
        TargetName::new(self.package.name(), self.name)
    }

    pub fn stable_hash<'a>(&self, ws: &'a Path) -> StepStableHash<'a> {
        StepStableHash(self.clone(), ws)
    }

    pub fn target(&self) -> Option<&TargetName> {
        if let Program::Target(name) = &self.program {
            Some(name)
        } else {
            None
        }
    }

    pub fn output_name(&self) -> PathBuf {
        let mut path = PathBuf::from(&self.name);
        path.set_extension("out");
        path
    }

    pub fn output_path(&self, layout: &Layout) -> PathBuf {
        let mut path = layout.output_dir(&self.package);
        path.push(self.output_name());
        path
    }

    pub fn dep_info_path(&self, layout: &Layout) -> PathBuf {
        let mut path = layout.fingerprint();
        path.push(&self.package.unique_name());
        path.push(&self.name);
        path.set_extension("d");
        path
    }
    
    pub fn fingerprint_path(&self, layout: &Layout) -> PathBuf {
        self.dep_info_path(layout).with_extension("hash")
    }

    // TODO: Check step syntax

    pub fn run<O: Write, E: Write + 'static>(
        &self, 
        cx: &Context,
        fingerprint: &mut Arc<Fingerprint>,
        mut stdout: MsgWriter<O>,
        mut stderr: MsgWriter<E>,
    ) -> IResult<ExitStatus> {        
        let program = match &self.program {
            Program::Target(name) => {
                let target = cx.units.get(name, &self.package).as_target().unwrap();
                target.output_path(cx.layout, cx.toolchain.target())
            }
            Program::Binary(path) => {
                // TODO: try find path in tools dir
                path.clone()
            }
            Program::Script { tool, .. } => {
                tool.clone()
            }
        };

        let mut cmd = Command::new(program);
        
        // If executing a script then the first argument is the script path
        if let Program::Script { script, .. } = &self.program {
            // TODO: try to find script in scripts dir
            cmd.arg(script);
        }

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(&self.args)
            .spawn()?;

        let buf_stderr = BufReader::new(child.stderr.take().unwrap());
        rayon::spawn(move || {
            for line in buf_stderr.split(b'\n').filter_map(|l| l.ok()) {
                drop(write!(stderr, "ccargo:warning="));
                drop(stderr.write_all(&line));
                drop(writeln!(stderr, ""));
            }
        });

        let mut rerun_paths = Vec::new();        
        let buf_stdout = BufReader::new(child.stdout.take().unwrap());
        for line in buf_stdout.split(b'\n').filter_map(|l| l.ok()) {
            match Message::parse(&line) {
                Ok(Message::RerunIfChanged(path)) => {
                    rerun_paths.push(path.to_path_buf());
                }
                Ok(Message::Raw(line)) => { 
                    drop(writeln!(stdout, "{}", line));
                }                
                Err(e) => {
                    // TODO: Better error message for parsing of step output
                    drop(writeln!(stdout, "Step `{}` output parse error: `{}`", self.full_name(), e));
                }
            }
        }

        let status = child.wait()?;
        
        let output = self.output_path(cx.layout);
        
        if !rerun_paths.is_empty() {
            Arc::make_mut(fingerprint).add_rerun_if_changed(output.clone(), rerun_paths);
        }

        paths::write(&output, b"")?;

        // TODO: Write dep-info for step (inputs, and objects if the user requested)

        Ok(status)
    }
}

impl std::ops::Deref for Step {
    type Target = StepInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Eq for Step {}

impl PartialEq for Step {
    fn eq(&self, other: &Step) -> bool {
        std::ptr::eq(&*self.0, &*other.0)
    }
}

impl std::hash::Hash for Step {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        std::ptr::hash(&*self.0, hasher)
    }
}

impl std::fmt::Debug for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}


pub struct StepStableHash<'a>(Step, &'a Path);

impl<'a> std::hash::Hash for StepStableHash<'a> {
    fn hash<S: std::hash::Hasher>(&self, state: &mut S) {
        self.0.name.hash(state);
        self.0.depends.hash(state);
        self.0.package.stable_hash(self.1).hash(state);
        self.0.args.hash(state);
        for v in self.0.inputs.iter() {
            v.strip_prefix(self.1).unwrap().hash(state);
        }
        for v in self.0.outputs.iter() {
            v.strip_prefix(self.1).unwrap_or(v).hash(state);
        }
        match &self.0.program {
            Program::Target(name) => {
                name.hash(state);
            }
            Program::Binary(path) => {
                path.strip_prefix(self.1).unwrap_or(&path).hash(state);
            }
            Program::Script { tool, script } => {
                tool.strip_prefix(self.1).unwrap_or(&tool).hash(state);
                script.strip_prefix(self.1).unwrap_or(&script).hash(state);
            }
        }
    }
}


#[derive(Debug)]
enum Message<'a> {
    Raw(&'a str),
    RerunIfChanged(&'a Path),    
}

impl<'a> Message<'a> {
    fn parse(line: &'a [u8]) -> IResult<Self> {
        let line = std::str::from_utf8(line)?;

        if let Some(rest) = line.strip_prefix("ccargo:") {
            if rest.starts_with("rerun-if-changed:") {
                Ok(Self::RerunIfChanged(Path::new(rest)))
            } else {
                anyhow::bail!("Invalid step directive `{}`", line)
            }
        } else {
            Ok(Self::Raw(line))
        }
    }
}
