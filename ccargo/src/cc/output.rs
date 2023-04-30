use super::{ToolKind, ToolFamily};
use crate::utils::{IResult, ByteFind, ColorString, Color, WriteColorExt};
use std::io::{Read, Write, BufRead, BufReader};
use std::path::PathBuf;
use std::ops::Range;


#[derive(Debug, Clone, Copy, Default)]
pub enum Kind {
    #[default]
    Warning,
    Error,
}


#[derive(Debug)]
pub enum Message {
    Header(Status),
    Body(ColorString),
    Status(ColorString),
    Extra(Extra),
}


#[derive(Debug)]
pub enum Extra {
    IncludePath(PathBuf),
}


#[derive(Debug, Default)]
pub struct Status {
    pub kind: Kind,
    pub loc: Loc,
    pub msg: ColorString,
    pub code: Option<ColorString>,
}


#[derive(Debug, Default)]
pub struct Loc {
    pub path: ColorString,
    pub line: Option<ColorString>,
    pub column: Option<ColorString>,
    pub func: Option<ColorString>,
}


pub struct MessageIter<R: Read> {
    kind: ToolKind,
    family: ToolFamily,
    windows: bool,
    colors: bool,
    buf: BufReader<R>,
    part: Vec<u8>,
    parser: Parser,
}

impl<R: Read> Iterator for MessageIter<R> {
    type Item = Message;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.part.clear();
            match self.buf.read_until(b'\n', &mut self.part) {
                Ok(0) | Err(..) => return None,
                Ok(_n) => {
                    if *self.part.last().unwrap() == b'\n' {
                        self.part.pop();
                    }
                    if *self.part.last().unwrap() == b'\r' {
                        self.part.pop();
                    }
                    let msg = match self.family {
                        ToolFamily::Msvc => {
                            self.parser.msvc(&self.part)
                        }
                        ToolFamily::Gnu => {
                            if let ToolKind::Linker = &self.kind {
                                self.parser.ld(&self.part, self.windows)
                            } else {
                                self.parser.gcc(&mut self.part, self.colors)
                            }
                        }
                        ToolFamily::Clang => {
                            if let ToolKind::Linker = &self.kind {
                                // clang on windows uses msvc linker
                                if self.windows { 
                                    self.parser.msvc(&self.part) 
                                } else { 
                                    self.parser.ld(&self.part, false) 
                                }
                            } else {
                                self.parser.clang(&mut self.part, self.colors)
                            }
                        }
                    };
                    if msg.is_some() {
                        return msg;
                    }
                }
            }
        }
    }
}


impl Message {
    pub fn iter<R: Read>(
        input: R, 
        kind: ToolKind,
        family: ToolFamily,
        windows: bool,
        colors: bool
    ) -> MessageIter<R> {
        MessageIter { 
            kind,
            family, 
            windows,
            colors, 
            buf: BufReader::new(input),
            part: Vec::default(), 
            parser: Parser::default(),
        }
    }

    pub fn print<W: Write>(&self, o: &mut W, colors: bool) -> IResult<()> {
        match self {
            Self::Extra(..) => {
                Ok(())
            }
            Self::Body(m) | Self::Status(m) => {
                if colors {
                    let mut m = m.clone();
                    m.reset_color()?;
                    o.write_all(&m)?;
                } else {
                    o.write_all(&m)?;
                }
                o.write(b"\n")?;
                Ok(())
            }
            Self::Header(s) => {
                let mut w = ColorString::new();
                if colors {
                    w.set_bold(Some(match s.kind {
                        Kind::Warning => Color::Yellow,
                        Kind::Error => Color::Red,
                    }))?;
                }
                w.write_all(match s.kind {
                    Kind::Warning => b"warning",
                    Kind::Error => b"error",
                })?;
                if let Some(code) = &s.code {
                    w.write(b" ")?;
                    w.write_all(&code)?;
                }
                if colors {
                    w.set_bold(None)?;
                }
                w.write(b": ")?;
                if colors {
                    w.reset_color()?;
                }
                
                w.write_all(&s.msg)?;
                
                if colors {
                    w.set_bold(Some(Color::Cyan))?;
                }
                w.write(b"\n   --> ")?;
                if colors {
                    w.reset_color()?;
                }
                
                w.write_all(&s.loc.path)?;
                if let Some(ln) = &s.loc.line {
                    w.write(b":")?;
                    w.write_all(&ln)?;
                }
                if let Some(col) = &s.loc.column {
                    w.write(b":")?;
                    w.write_all(&col)?;
                }
                if let Some(func) = &s.loc.func {
                    w.write_all(b" in function `")?;
                    w.write_all(&func)?;
                    w.write_all(b"`")?;
                }

                if colors {
                    w.reset_color()?;
                }
                w.push('\n');
                o.write_all(&w)?;
                Ok(())
            }
        }
    }
}


#[derive(Default)]
struct Parser {
    kind: Option<Kind>,
    // gcc
    func: Option<ColorString>,
    // clang
    last_line: Option<usize>,
    // msvc
    warnings_as_errors: bool,
}

impl Loc {
    fn new_path(path: &[u8]) -> Self {
        Self { path: ColorString::from(path), ..Default::default() }
    }

    fn new(path: &[u8], line: &[u8], column: Option<&[u8]>) -> Self {
        Self { 
            path: ColorString::from(path), 
            line: Some(ColorString::from(line)), 
            column: column.map(ColorString::from),
            func: None,
        }
    }
}

impl Parser {
    fn msvc(&mut self, line: &[u8]) -> Option<Message> {
        // Useless info we want to strip
        if line.starts_with(b"Generating Code...") 
            || line.starts_with(b"   Creating library")
            // Source file names are also ignored
            || !line.contains(&b' ')
        {
            return None;
        }
        // Output from `-showIncludes` is combined with warnings/errors, *sigh*
        else if let Some(rest) = line.strip_prefix(b"Note: including file:") {
            return if let Ok(s) = std::str::from_utf8(rest) {
                Some(Message::Extra(Extra::IncludePath(PathBuf::from(s.trim()))))
            } else {
                None
            };
        }
        // Now all lines should have the standard MSVC message format
        // Message format for MSVC tools (taken from MSVC docs)
        //  `Origin : Subcategory Category Code : Text`
        let col0 = line.find(b"):")? + 1;
        let col1 = 1 + col0 + line[col0 + 1..].find(b':')?;
        let loc = &line[..col0];
        let status = &line[col0 + 2..col1];
        let body = &line[col1 + 2..];
        let code = status.split(|v| *v == b' ').last();
        let kind = if status.rfind(b"error").is_some() { 
            self.warnings_as_errors |= &line.ends_with(b"treated as an error");
            Kind::Error 
        } else if status.rfind(b"warning").is_some() { 
            if self.warnings_as_errors {
                Kind::Error
            } else {
                Kind::Warning
            }
        } else {
            return Some(Message::Body(ColorString::from(line)));
        };

        Some(Message::Header(Status{
            kind,
            msg: ColorString::from(body),
            code: code.map(ColorString::from),
            loc: Self::loc_msvc(loc),
        }))
    }

    fn clang(&mut self, line: &mut [u8], colors: bool) -> Option<Message> {
        if colors {
            replace_warning_color(line);
            // clang uses a different color to highlight the problematic section
            // of code than the standard error/warning colors, so we replace it
            if let Some(k) = self.kind.as_ref() {
                replace_color(
                    GREEN,
                    if let Kind::Error = k { RED } else { YELLOW },
                    line
                )
            }
        }
    
        let msg = self.gcc_clang(line);
        Some(match msg {
            // convert `N errors and N warnings generated.` line into status message
            Message::Body(m) if m.ends_with(b" generated.") => {
                Message::Status(m)
            }
            // we want to add a line prefix to body messages to match gcc
            Message::Body(m) if m.find(b"    ").is_some() => {
                let prefix = if let Some(line) = self.last_line.take() {
                    format!("{line:5} | ")
                } else {
                    format!("      | ")
                };
                let mut prefixed = ColorString::from(prefix.as_bytes());
                prefixed.push_bytes(&m);
                Message::Body(prefixed)
            }
            // record line from header message
            Message::Header(s) => {
                self.last_line = s.loc.line.as_ref()
                    .and_then(|v| std::str::from_utf8(&v).ok())
                    .and_then(|v| v.parse::<usize>().ok());
                Message::Header(s)
            }
            m => m
        })
    }

    fn gcc(&mut self, line: &mut [u8], colors: bool) -> Option<Message> {
        if colors {
            replace_warning_color(line);
        }

        // `in function ...` can come before the error, but is part of the error,
        // so we need to store it and use it later
        if let Some(func) = Self::func_name(line) {
            self.func = Some(ColorString::from(func));
            return None;
        }

        Some(self.gcc_clang(line))
    }
    
    fn ld(&mut self, line: &[u8], windows: bool) -> Option<Message> {
        // `in function ...` can come before the error, but is part of the error,
        // so we need to store it and use it later
        if let Some(func) = Self::func_name(line) {
            self.func = Some(ColorString::from(&func[1..func.len()-1]));
            return None;
        }

        // the path for windows gnu `ld.exe` can be quite long, so we shorten it
        let line = if let (true, Some(i)) = (windows, line.find(b"/ld.exe: ")) {
            &line[i+1..]
        } else {
            &line[..]
        };

        let path_end = line.find(b":(")?;
        let section_end = 2 + path_end + line[path_end+2..].find(b"):")?;
        Some(Message::Header(Status{
            kind: Kind::Error,
            code: Some(ColorString::from(&line[path_end+2..section_end])),
            msg: ColorString::from(&line[section_end+2..]),
            loc: Loc{
                path: ColorString::from(&line[..path_end]),
                func: self.func.clone(),
                ..Default::default()
            }
        }))
    }

    fn gcc_clang(&mut self, line: &[u8]) -> Message {
        let (kind, r) = if let Some(i) = line.find(b"error:") {
            (Kind::Error, Range{start: i, end: i + b"error: ".len()})
        } else if let Some(i) = line.find(b"warning:") {
            (Kind::Warning, Range{start: i, end: i + b"warning: ".len()})
        } else {
            return Message::Body(ColorString::from(line));
        };
        self.kind = Some(kind);

        let mut status = Status::default();
        status.kind = kind;
        status.msg = ColorString::from(&line[r.end..]);
        
        let loc_end = line[..r.start].rfind(b':').unwrap();
        status.loc = Self::loc_gcc_clang(&line[..loc_end]);
        status.loc.func = self.func.clone();

        Message::Header(status)
    }

    fn func_name(line: &[u8]) -> Option<&[u8]> {
        let i = line.find(b"n function ")?;
        let func = &line[i + b"n function ".len()..line.len()-1];
        Some(trim_colors(func))
    }

    fn loc_gcc_clang(mut loc: &[u8]) -> Loc {
        // Strip color prefix from path
        loc = trim_colors(loc);

        // Find column/line
        let col_begin = if let Some(i) = loc.rfind(b':') { i } else {
            return Loc::new_path(loc);
        };
        let ln_begin = if let Some(i) = loc[..col_begin].rfind(b':') { i } else {
            return Loc::new(&loc[..col_begin], &loc[col_begin+1..], None);
        };
        // Full line/column info
        Loc::new(
            &loc[..ln_begin],
            &loc[ln_begin+1..col_begin],
            Some(&loc[col_begin+1..])
        )
    }

    fn loc_msvc(loc: &[u8]) -> Loc {
        // Origin can have a suffix
        //  (line)
        //  (line,col)
        //  (line,col-col)
        //  (line,col,line,col)
        let (path, line, column) = if let Some(ln_begin) = loc.rfind(b'(').map(|v| v + 1) {
            let loc_end = loc.len() - 1;
            let (ln_end, col_begin) = if let Some(ln_end) = loc.rfind(b',') {
                (ln_end, Some(ln_end+1))
            } else {
                (loc_end, None)
            };
            (
                &loc[..ln_begin-1],
                Some(&loc[ln_begin..ln_end]),
                col_begin.map(|i| &loc[i..loc_end])
            )
        } else {
            (loc, None, None)
        };
        Loc {
            path: ColorString::from(path),
            line: line.map(ColorString::from),
            column: column.map(ColorString::from),
            func: None,
        }
    }
}


// Ansi color codes without escape and other info, used for replacing
const RED: u8 = b'1';
const GREEN: u8 = b'2';
const YELLOW: u8 = b'3';
const MAGENTA: u8 = b'5';


// warnings in clang ang gcc are magenta, and we want all warnings to be yellow
fn replace_warning_color(haystack: &mut [u8]) {
    replace_color(MAGENTA, YELLOW, haystack);
}


// Replaces an ANSI color in the given slice
fn replace_color(color: u8, replacement: u8, haystack: &mut [u8]) {
    let bytes = [b';', b'3', color, b'm'];
    let mut offset = 0;
    while let Some(i) = haystack[offset..].find(&bytes) {
        haystack[offset + i + 2] = replacement;
        offset += i;
    }
}


// Trim colors from start/end of a section
fn trim_colors(s: &[u8]) -> &[u8] {
    const PARTS: &'static [&'static [u8]] = &[
        &[b'\x1b', b'[', b'K'],
        &[b'\x1b', b'[', b'1', b'm'],
    ];
    for part in PARTS {
        if let Some(begin) = s.find(*part) {
            let begin = begin + part.len();
            return if let Some(end) = s[begin..].find(b'\x1b') {
                &s[begin..begin+end]
            } else {
                &s[begin..]
            }
        }
    }
    &s[..]
}
