use crate::utils::{IResult, WriteColorExt};

use std::fmt;
use std::io::prelude::*;

use termcolor::Color::{Cyan, Green};
use termcolor::{self, Color, ColorSpec, StandardStream, WriteColor};


/// Whether messages should use color output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Force color output
    Always,
    /// Force disable color output
    Never,
    /// Intelligently guess whether to use color output
    Auto,
}


/// The requested verbosity of output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Verbose,
    Normal,
    Quiet,
}


/// An abstraction around console output that remembers preferences for output
/// verbosity and color.
pub struct Shell {
    /// Wrapper around stdout/stderr. This helps with supporting sending
    /// output to a memory buffer which is useful for tests.
    output: ShellOut,
    /// How verbose messages should be.
    verbosity: Verbosity,
}


/// A `Write`able object, either with or without color support
enum ShellOut {
    /// A plain write object without color support
    Write(Box<dyn Write>),
    /// Color-enabled stdio, with information on whether color should be used
    Stream {
        stdout: StandardStream,
        stderr: StandardStream,
        stderr_tty: bool,
        color_choice: ColorChoice,
    },
}

impl Shell {
    /// Creates a new shell (color choice and verbosity), defaulting to 'auto' color and verbose
    /// output.
    pub fn new() -> Shell {
        let auto_clr = ColorChoice::Auto;
        Shell {
            output: ShellOut::Stream {
                stdout: StandardStream::stdout(auto_clr.to_termcolor(atty::Stream::Stdout)),
                stderr: StandardStream::stderr(auto_clr.to_termcolor(atty::Stream::Stderr)),
                stderr_tty: atty::is(atty::Stream::Stderr),
                color_choice: auto_clr,
            },
            verbosity: Verbosity::Verbose,
        }
    }

    /// Creates a shell from a plain writable object, with no color, and max verbosity.
    pub fn from_write<W: Write + 'static>(out: W) -> Shell {
        Shell {
            output: ShellOut::Write(Box::new(out)),
            verbosity: Verbosity::Verbose,
        }
    }

    /// Prints a red 'error' message.
    pub fn error<T: fmt::Display>(&mut self, message: T) -> IResult<()> {
        self.output.stderr_status("error", Some(&message))
    }

    /// Prints an amber 'warning' message.
    pub fn warn<T: fmt::Display>(&mut self, message: T) -> IResult<()> {
        self.stderr_status("warning", Some(&message))
    }

    /// Prints a cyan 'note' message.
    pub fn note<T: fmt::Display>(&mut self, message: T) -> IResult<()> {
        self.stderr_status("note", Some(&message))
    }

    /// Shortcut to right-align and color green a status message.
    pub fn status<T, U>(&mut self, status: T, message: U) -> IResult<()>
    where
        T: fmt::Display,
        U: fmt::Display,
    {
        self.print_justified(&status, Some(&message), Green)
    }

    /// Shortcut to right-align a status message.
    pub fn status_with_color<T, U>(&mut self, status: T, message: U, color: Color) -> IResult<()>
    where
        T: fmt::Display,
        U: fmt::Display,
    {
        self.print_justified(&status, Some(&message), color)
    }

    /// Shortcut to right-align and color cyan a status header with no message.
    pub fn status_header<T>(&mut self, status: T) -> IResult<()>
    where
        T: fmt::Display,
    {
        self.print_justified(&status, None, Cyan)
    }

    /// Runs the callback if we are not in verbose mode.
    pub fn concise<F>(&mut self, mut callback: F) -> IResult<()>
    where
        F: FnMut(&mut Shell) -> IResult<()>,
    {
        match self.verbosity {
            Verbosity::Verbose => Ok(()),
            _ => callback(self),
        }
    }

    /// Runs the callback only if we are in verbose mode.
    pub fn verbose<F>(&mut self, mut callback: F) -> IResult<()>
    where
        F: FnMut(&mut Shell) -> IResult<()>,
    {
        match self.verbosity {
            Verbosity::Verbose => callback(self),
            _ => Ok(()),
        }
    }

    /// Gets a reference to the underlying stdout writer.
    pub fn out(&mut self) -> &mut dyn Write {
        self.output.stdout()
    }

    /// Gets a reference to the underlying stderr writer.
    pub fn err(&mut self) -> &mut dyn Write {
        self.output.stderr()
    }

    /// Whether stdout supports color.
    pub fn out_supports_color(&self) -> bool {
        match &self.output {
            ShellOut::Write(_) => false,
            ShellOut::Stream { stdout, .. } => stdout.supports_color(),
        }
    }

    /// Whether stderr supports color.
    pub fn err_supports_color(&self) -> bool {
        match &self.output {
            ShellOut::Write(_) => false,
            ShellOut::Stream { stderr, .. } => stderr.supports_color(),
        }
    }

    /// Returns `true` if stderr is a tty (i.e. intended to be read by a human).
    pub fn is_err_tty(&self) -> bool {
        match self.output {
            ShellOut::Stream { stderr_tty, .. } => stderr_tty,
            _ => false,
        }
    }

    /// Gets the verbosity of the shell.
    pub fn verbosity(&self) -> Verbosity {
        self.verbosity
    }
    
    /// Updates the verbosity of the shell.
    pub fn set_verbosity(&mut self, verbosity: Verbosity) {
        self.verbosity = verbosity;
    }

    /// Gets the current color choice.
    ///
    /// If we are not using a color stream, this will always return `Never`, even if the color
    /// choice has been set to something else.
    pub fn color_choice(&self) -> ColorChoice {
        match self.output {
            ShellOut::Stream { color_choice, .. } => color_choice,
            ShellOut::Write(_) => ColorChoice::Never,
        }
    }

    /// Updates the color choice (always, never, or auto) from a string..
    pub fn set_color_choice(&mut self, color: Option<&str>) -> IResult<()> {
        if let ShellOut::Stream {
            ref mut stdout,
            ref mut stderr,
            ref mut color_choice,
            ..
        } = self.output
        {
            let cfg = match color {
                Some("always") => ColorChoice::Always,
                Some("never") => ColorChoice::Never,

                Some("auto") | None => ColorChoice::Auto,

                Some(arg) => anyhow::bail!(
                    "argument for --color must be auto, always, or \
                     never, but found `{}`",
                    arg
                ),
            };
            *color_choice = cfg;
            *stdout = StandardStream::stdout(cfg.to_termcolor(atty::Stream::Stdout));
            *stderr = StandardStream::stderr(cfg.to_termcolor(atty::Stream::Stderr));
        }
        Ok(())
    }

    /// Prints a message, where the status will have `color` color. The message follows without color.
    fn stderr_status(
        &mut self, 
        status: &str, 
        msg: Option<&dyn fmt::Display>,
    ) -> IResult<()> {
        match self.verbosity {
            Verbosity::Quiet => Ok(()),
            _ => self.output.stderr_status(status, msg)
        }
    }
    
    /// Prints a message, where the status will have `color` color, and will be justified. The message follows without color.
    fn print_justified(
        &mut self, 
        status: &dyn fmt::Display, 
        msg: Option<&dyn fmt::Display>, 
        color: Color
    ) -> IResult<()> {
        match self.verbosity {
            Verbosity::Quiet => Ok(()),
            _ => self.output.stderr_status_justified(status, msg, color)
        }
    }
}

impl ShellOut {
    /// Prints out a message with a status. The status comes first, and is bold plus the given color.
    fn stderr_status(&mut self, status: &str, msg: Option<&dyn fmt::Display>) -> IResult<()> {
        match *self {
            ShellOut::Stream { ref mut stderr, .. } => {
                stderr.reset()?;
                stderr.write_status(status, stderr.supports_color())?;
                match msg {
                    Some(msg) => writeln!(stderr, " {}", msg)?,
                    None => write!(stderr, " ")?,
                }
            }
            ShellOut::Write( ref mut w ) => {
                write!(w, "{}: ", status)?;
                if let Some(m) = msg {
                    writeln!(w, "{}", m)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Prints out a message with a status. The status comes first, and is bold plus the given
    /// color. The status will be justified, where the max width that will right align is 12 chars.
    fn stderr_status_justified(
        &mut self,
        status: &dyn fmt::Display,
        msg: Option<&dyn fmt::Display>,
        color: Color,
    ) -> IResult<()> {
        match *self {
            ShellOut::Stream { ref mut stderr, .. } => {
                stderr.reset()?;
                stderr.set_spec(ColorSpec::new().set_bold(true).set_fg(Some(color)))?;
                write!(stderr, "{:>12}", status)?;
                stderr.reset()?;
                match msg {
                    Some(msg) => writeln!(stderr, " {}", msg)?,
                    None => write!(stderr, " ")?,
                }
            }
            ShellOut::Write(ref mut w) => {
                write!(w, "{:>12}", status)?;
                match msg {
                    Some(msg) => writeln!(w, " {}", msg)?,
                    None => write!(w, " ")?,
                }
            }
        }
        Ok(())
    }

    /// Gets stdout as a `io::Write`.
    fn stdout(&mut self) -> &mut dyn Write {
        match *self {
            ShellOut::Stream { ref mut stdout, .. } => stdout,
            ShellOut::Write(ref mut w) => w,
        }
    }

    /// Gets stderr as a `io::Write`.
    fn stderr(&mut self) -> &mut dyn Write {
        match *self {
            ShellOut::Stream { ref mut stderr, .. } => stderr,
            ShellOut::Write(ref mut w) => w,
        }
    }
}

impl ColorChoice {
    /// Converts our color choice to termcolor's version.
    fn to_termcolor(self, stream: atty::Stream) -> termcolor::ColorChoice {
        match self {
            ColorChoice::Always => termcolor::ColorChoice::Always,
            ColorChoice::Never => termcolor::ColorChoice::Never,
            ColorChoice::Auto => {
                if atty::is(stream) {
                    termcolor::ColorChoice::Auto
                } else {
                    termcolor::ColorChoice::Never
                }
            }
        }
    }
}

impl fmt::Debug for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.output {
            ShellOut::Write(_) => f
                .debug_struct("Shell")
                .field("verbosity", &self.verbosity)
                .finish(),
            ShellOut::Stream { color_choice, .. } => f
                .debug_struct("Shell")
                .field("verbosity", &self.verbosity)
                .field("color_choice", &color_choice)
                .finish(),
        }
    }
}
