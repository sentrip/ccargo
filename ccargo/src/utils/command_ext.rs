use crate::utils::IResult;

pub trait CommandExt {
    fn exec_replace(&mut self) -> IResult<()>;
}

impl CommandExt for std::process::Command {
    fn exec_replace(&mut self) -> IResult<()> {
        imp::exec_replace(self)
    }
}

#[cfg(windows)]
mod imp {
    use anyhow::Result;
    use winapi::shared::minwindef::{BOOL, DWORD, FALSE, TRUE};
    use winapi::um::consoleapi::SetConsoleCtrlHandler;
    
    unsafe extern "system" fn ctrlc_handler(_: DWORD) -> BOOL {
        // Do nothing; let the child process handle it.
        TRUE
    }
    
    pub fn exec_replace(cmd: &mut std::process::Command) -> Result<()> {
        // Set Ctrl-C handler for parent process
        unsafe {
            if SetConsoleCtrlHandler(Some(ctrlc_handler), TRUE) == FALSE {
                anyhow::bail!("Could not set Ctrl-C handler.");
            }
        }

        // Just execute the process as normal.
        cmd.spawn()?.wait()?;
        
        Ok(())
    }

}


#[cfg(unix)]
mod imp {
    use anyhow::Result;
    use std::os::unix::process::CommandExt;

    pub fn exec_replace(cmd: &mut std::process::Command) -> Result<()> {
        let error = command.exec();
        Err(anyhow::Error::from(error).context(format!("could not execute process {:?}", cmd)))
    }
}

