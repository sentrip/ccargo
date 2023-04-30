use super::Error;
use std::io::Read;
use std::process::{Command, Child, Stdio, ExitStatus};


// Run command and return command's stdout output
pub(super) fn run_stdout(cmd: &mut Command, program: &str) -> Result<Vec<u8>, Error> {
    let mut child = run(cmd, program)?;
    let output = read_output(&mut child.stdout);
    let status = wait_child(cmd, program, &mut child)?;
    verify_status(cmd, program, status)?;
    Ok(output)
}


// Run command and return command's stderr output
pub(super) fn run_stderr(cmd: &mut Command, program: &str) -> Result<Vec<u8>, Error> {
    let mut child = run(cmd, program)?;
    let output = read_output(&mut child.stderr);
    let status = wait_child(cmd, program, &mut child)?;
    verify_status(cmd, program, status)?;
    Ok(output)
}


// Run command and return child that can be waited on
pub(super) fn run(cmd: &mut Command, program: &str) -> Result<Child, Error> {
    match cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn() 
    {
        Ok(output) => Ok(output),

        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => 
            Err(Error::tool_not_found(format!(
                "Failed to find tool. Is `{}` installed?", 
                program
            ))),
        
        Err(ref e) => 
            Err(Error::tool_exec(format!(
                "Command `{}` failed to start: {}\nArgs:\n{:?}",
                program, e, cmd
            ))),
    }
}


// Verify status code of command execution
pub(super) fn verify_status(cmd: &Command, program: &str, status: ExitStatus) -> Result<(), Error> {
    if status.success() {
        Ok(())
    } else {
        Err(Error::tool_exec(format!(
            "Command `{}` returned with non-zero status code: {}\nArgs:\n{:?}",
            program, status, cmd
        )))
    }
}


// Wait for child to finish executing
pub(super) fn wait_child(cmd: &Command, program: &str, child: &mut Child) -> Result<ExitStatus, Error> {
    match child.wait() {
        Ok(s) => Ok(s),
        Err(ref e) => 
            Err(Error::tool_exec(format!(
                "Command `{}` failed to run: {}\nArgs:\n{:?}",
                program, e, cmd
            ))),
    }
}

fn read_output<R: Read>(input: &mut Option<R>) -> Vec<u8> {
    let mut output = Vec::new();
    input
        .take()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    output
}
