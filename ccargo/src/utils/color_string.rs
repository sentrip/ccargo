use crate::utils::IResult;

pub use termcolor::Color;

use std::fmt;
use termcolor::{Ansi, ColorSpec, WriteColor};


pub trait WriteColorExt: std::io::Write {
    fn set_spec(&mut self, spec: &ColorSpec) -> IResult<()>;

    fn reset_color(&mut self) -> IResult<()>;
    
    fn set_color(&mut self, color: Color) -> IResult<()> {
        self.set_spec(ColorSpec::new().set_fg(Some(color)))
    }

    fn set_bold(&mut self, color: Option<Color>) -> IResult<()> {
        self.set_spec(ColorSpec::new().set_fg(color).set_bold(true))
    }

    fn write_color<B: AsRef<[u8]>>(&mut self, buf: B, color: Color) -> IResult<()> {
        self.set_color(color)?;
        self.write_all(buf.as_ref())?;
        self.reset_color()
    }
    
    fn write_bold<B: AsRef<[u8]>>(&mut self, buf: B, color: Option<Color>) -> IResult<()> {
        self.set_bold(color)?;
        self.write_all(buf.as_ref())?;
        self.reset_color()
    }

    fn write_status(&mut self, status: &str, colored: bool) -> IResult<()> {
        let color = match status.chars().next() {
            Some('w') => Color::Yellow,
            Some('e') => Color::Red,
            Some('n') => Color::Cyan,
            _ => Color::White,
        };
        if colored {
            self.write_bold(status, Some(color))?;
            self.write_bold(b":", None)?;
        } else {
            self.write_all(status.as_bytes())?;
            self.write(b":")?;
        }
        Ok(())
    }

    /// Prints out a message with a status. The status comes first, and is bold plus the given
    /// color. The status will be justified, where the max width that will right align is 12 chars.
    fn write_status_justified(
        &mut self,
        status: &dyn fmt::Display,
        msg: Option<&dyn fmt::Display>,
        color: Color,
    ) -> IResult<()> {
        self.reset_color()?;
        self.set_spec(ColorSpec::new().set_bold(true).set_fg(Some(color)))?;
        write!(self, "{:>12}", status)?;
        self.reset_color()?;
        match msg {
            Some(msg) => writeln!(self, " {}", msg)?,
            None => write!(self, " ")?,
        }
        Ok(())
    }
}

impl<T> WriteColorExt for T where T: WriteColor {
    fn set_spec(&mut self, spec: &ColorSpec) -> IResult<()> {
        self.set_color(spec)?;
        Ok(())
    }
    fn reset_color(&mut self) -> IResult<()> {
        self.reset()?;
        Ok(())
    }    
}


pub struct ColorString(Ansi<Vec<u8>>);

impl ColorString {
    pub fn new() -> Self {
        Self(termcolor::Ansi::new(Vec::new()))
    }

    pub fn is_empty(&self) -> bool {
        self.get().is_empty()
    }

    pub fn len(&self) -> usize {
        self.get().len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.get().as_slice()
    }

    pub fn clear(&mut self) {
        self.get_mut().clear();
    }

    pub fn push(&mut self, c: char) {
        let mut bytes = [0; 4];
        self.push_str(c.encode_utf8(&mut bytes))
    }

    pub fn push_bytes(&mut self, s: &[u8]) {
        self.get_mut().extend_from_slice(s)
    }

    pub fn push_str(&mut self, s: &str) {
        self.push_bytes(s.as_bytes())
    }

    pub fn to_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.get())
    }
    
    pub unsafe fn to_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(&self.get())
    }

    fn get(&self) -> &Vec<u8> {
        self.0.get_ref()
    }
    
    fn get_mut(&mut self) -> &mut Vec<u8> {
        self.0.get_mut()
    }
}

impl Eq for ColorString {}

impl Ord for ColorString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.get().cmp(other.get())
    }
}

impl PartialEq for ColorString {
    fn eq(&self, other: &Self) -> bool {
        self.get().eq(other.get())
    }
}

impl PartialOrd for ColorString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.get().partial_cmp(other.get())
    }
}

impl Clone for ColorString {
    fn clone(&self) -> Self {
        Self(termcolor::Ansi::new(self.get().clone()))
    }
}

impl std::default::Default for ColorString {
    fn default() -> Self { 
        Self::new() 
    }
}

impl std::hash::Hash for ColorString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state)
    }
}

impl std::ops::Deref for ColorString {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for ColorString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl std::convert::From<Vec<u8>> for ColorString {
    fn from(value: Vec<u8>) -> Self {
        Self(termcolor::Ansi::new(value))
    }
}

impl std::convert::From<&[u8]> for ColorString {
    fn from(value: &[u8]) -> Self {
        Self(termcolor::Ansi::new(Vec::from(value)))
    }
}

impl std::convert::Into<Vec<u8>> for ColorString {
    fn into(self) -> Vec<u8> {
        self.0.into_inner()
    }
}

impl std::fmt::Debug for ColorString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { self.to_str_unchecked() }.fmt(f)
    }
}

impl std::fmt::Display for ColorString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe { self.to_str_unchecked() }.fmt(f)
    }
}

impl std::io::Write for ColorString {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl termcolor::WriteColor for ColorString {
    fn supports_color(&self) -> bool {
        true
    }
    
    fn set_color(&mut self, spec: &ColorSpec) -> std::io::Result<()> {
        WriteColor::set_color(&mut self.0, spec)
    }

    fn reset(&mut self) -> std::io::Result<()> {
        WriteColor::reset(&mut self.0)
    }
}
