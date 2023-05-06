use crate::core::{TargetName, PackageId, Layout, Context, fingerprint::DepInfo};
use crate::utils::{IResult, InternedString, MsgWriter, paths, BinarySerialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;


#[derive(Debug, Hash)]
pub enum Program {
    Binary(PathBuf),
    Target(TargetName),
    Script{tool: PathBuf, script: PathBuf},
}

impl From<&str> for Program {
    fn from(command: &str) -> Self {
        if let Ok(name) = TargetName::from_str(command) {
            Program::Target(name)
        } else {
            let script = PathBuf::from(command);
            match script.extension().and_then(|x| x.to_str()) {
                Some("py") => Program::Script{script, tool: "python".into()},
                Some("sh") => Program::Script{script, tool: "sh".into()},
                Some("bat") => Program::Script{script, tool: "cmd.exe".into()},
                Some("ps1") => Program::Script{script, tool: "powershell.exe".into()},
                _ => Program::Binary(script),
            }
        }
    }
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
                // TODO: try to find script in tools dir
                tool.clone()
            }
        };
        
        let mut cmd = Command::new(program);
        
        // If executing a script then the first argument is the script path
        if let Program::Script { script, .. } = &self.program {
            cmd.arg(script);
        }
        
        let root = self.package.root();
        let mut child = cmd
            .args(&self.args)
            .current_dir(root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // TODO: Cache output for step and replay when fresh?
        let buf_stderr = BufReader::new(child.stderr.take().unwrap());
        rayon::spawn(move || {
            for mut line in buf_stderr.split(b'\n').filter_map(|l| l.ok()) {
                if *line.last().unwrap() == b'\r' { 
                    line.pop(); 
                }
                drop(write!(stderr, "ccargo:warning="));
                drop(stderr.write_all(&line));
                drop(writeln!(stderr, ""));
            }
        });

        let mut dep_info = DepInfo::default();
        let buf_stdout = BufReader::new(child.stdout.take().unwrap());
        for mut line in buf_stdout.split(b'\n').filter_map(|l| l.ok()) {
            if *line.last().unwrap() == b'\r' { 
                line.pop();
            }
            // TODO: Better error message for parsing of step output
            match Message::parse(&line) {
                Err(e) => {
                    drop(writeln!(stdout, "Step `{}` output parse error: `{}`", self.full_name(), e));
                }
                Ok(Message::Raw(line)) => { 
                    drop(writeln!(stdout, "{}", line));
                }
                Ok(Message::RerunIfChanged(path)) => {
                    if let Ok(rel) = paths::abs(path, root).strip_prefix(root) {
                        dep_info.add_pkg_relative(rel.to_path_buf());
                    } else {
                        drop(writeln!(stdout, "Path `{:?}` was ignored as it is outside of the package root: `{:?}`", path, root));
                    }
                }
            }
        }

        let status = child.wait()?;

        let output = self.output_path(cx.layout);
        paths::write_create_all(&output, b"")?;

        if !dep_info.is_empty() {
            paths::write_create_all(self.dep_info_path(cx.layout), dep_info.to_bytes())?;
        }

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
            if let Some(path) = rest.strip_prefix("rerun-if-changed:") {
                Ok(Self::RerunIfChanged(Path::new(path)))
            } else {
                anyhow::bail!("Invalid step directive `{}`", line)
            }
        } else {
            Ok(Self::Raw(line))
        }
    }
}
