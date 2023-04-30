use crate::utils::{IResult, Shell, ccargo_home};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use anyhow::Context;


/// Configuration information for ccargo. This is not specific to a build, it is information
/// relating to ccargo itself.
#[derive(Debug)]
pub struct Config {
    /// The location of the user's CCargo home directory. OS-dependent.
    home_path: PathBuf,
    /// The current working directory of ccargo
    cwd: PathBuf,
    /// Information about how to write messages to the shell
    shell: Mutex<Shell>,
    /// Whether we are printing extra verbose messages
    extra_verbose: bool,
    /// Creation time of this config, used to output the total build time
    creation_time: Instant,
}

impl Config {
    /// Creates a new config instance.
    ///
    /// This is typically used for tests or other special cases. `default` is
    /// preferred otherwise.
    ///
    /// This does only minimal initialization. In particular, it does not load
    /// any config files from disk. Those will be loaded lazily as-needed.
    pub fn new(shell: Shell, cwd: PathBuf, homedir: PathBuf) -> Config {
        Config {
            home_path: homedir,
            shell: Mutex::new(shell),
            cwd,
            extra_verbose: false,
            creation_time: Instant::now(),
        }
    }

    /// Creates a new Config instance, with all default settings.
    ///
    /// This does only minimal initialization. In particular, it does not load
    /// any config files from disk. Those will be loaded lazily as-needed.
    pub fn default() -> IResult<Config> {
        let shell = Shell::new();
        let cwd = std::env::current_dir()
            .with_context(|| "couldn't get the current directory of the process")?;
        let homedir = ccargo_home()?;
        Ok(Config::new(shell, cwd, homedir))
    }

    /// The current working directory.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    
    /// Gets the user's CCargo home directory (OS-dependent).
    pub fn home(&self) -> &Path {
        &self.home_path
    }
    
    /// Gets a reference to the shell, e.g., for writing error messages.
    pub fn shell(&self) -> MutexGuard<Shell> {
        self.shell.lock().unwrap()
    }
    
    pub fn extra_verbose(&self) -> bool {
        self.extra_verbose
    }

    pub fn creation_time(&self) -> Instant {
        self.creation_time
    }
}
