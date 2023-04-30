

/// Represents the types of errors that may occur while building C/C++ code
#[derive(Debug)]
pub enum ErrorKind {
    /// Error occurred while performing I/O.
    IOError,
    /// Invalid architecture supplied.
    ArchitectureInvalid,
    /// Environment variable not found, with the var in question as extra info.
    EnvVarNotFound,
    /// Error occurred while using external tools (ie: invocation of compiler).
    ToolExecError,
    /// Error occurred due to missing external tools.
    ToolNotFound,
    /// One of the function arguments failed validation.
    InvalidArgument,
}

/// Represents an internal error that occurred, with an explanation.
#[derive(Debug)]
pub struct Error {
    /// Describes the kind of error that occurred.
    kind: ErrorKind,
    /// More explanation of error that occurred.
    message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: &str) -> Self {
        Self { kind, message: message.to_owned() }
    }
    pub fn io(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::IOError, message.as_ref())
    }
    pub fn invalid_arch(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::ArchitectureInvalid, message.as_ref())
    }
    pub fn invalid_arg(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::InvalidArgument, message.as_ref())
    }
    pub fn env_not_found(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::EnvVarNotFound, message.as_ref())
    }
    pub fn tool_not_found(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::ToolNotFound, message.as_ref())
    }
    pub fn tool_exec(message: impl AsRef<str>) -> Self {
        Self::new(ErrorKind::ToolExecError, message.as_ref())
    }
}


impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error{kind: ErrorKind::IOError, message: format!("{}", e)}
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}
