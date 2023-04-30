use crate::Cfg;
use std::io;
use std::process::Command;
use std::str::FromStr;
use std::thread;

#[derive(Debug)]
pub struct RustcTarget {
    target: String,
    cfgs: Vec<Cfg>,
}

impl RustcTarget {
    pub fn new(target: String, cfgs: Vec<Cfg>) -> Self {
        Self{target, cfgs}
    }
    
    pub fn target(&self) -> &str {
        &self.target
    }
    
    pub fn cfgs(&self) -> &[Cfg] {
        &self.cfgs
    }

    pub fn detect_sync() -> io::Result<Self> {
        Ok(Self::new(Self::get_target()?, Self::get_cfgs()?))
    }
    
    pub fn detect() -> io::Result<Self> {
        let t = thread::spawn(|| Self::get_target());
        let c = thread::spawn(|| Self::get_cfgs());
        let rt = t.join().map_err(thread_error)?;
        let rc = c.join().map_err(thread_error)?;
        Ok(Self::new(rt?, rc?))
    }

    fn get_target() -> io::Result<String> {
        let output = Command::new("rustc")
            .arg("-Vv")
            .output()?;
        let stdout = String::from_utf8(output.stdout).unwrap();
        for line in stdout.lines() {
            if line.starts_with("host: ") {
                return Ok(String::from(&line[6..]));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData, 
            format!("rustc -Vv returned invalid data `{}`", stdout)
        ))
    }
    
    fn get_cfgs() -> io::Result<Vec<Cfg>> {
        let output = Command::new("rustc")
            .arg("--print=cfg")
            .output()?;
        let stdout = String::from_utf8(output.stdout).unwrap();
        let cfgs = stdout
            .lines()
            .map(|line| Cfg::from_str(line).unwrap())
            .collect();
        Ok(cfgs)
    }
}

fn thread_error(err: impl std::fmt::Debug) -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, format!("{:?}", err))
}
