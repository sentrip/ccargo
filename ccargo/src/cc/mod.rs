mod build;
pub use build::{BinType, Build, Object, Output, OutputMode, Artifact};

pub mod cmd;
pub mod dep_info;

mod error;
pub use error::{Error, ErrorKind};

mod options;
pub use options::*;

pub mod output;
pub use output::Message;

mod platform;
pub use platform::{RustcTarget, host_platform, host_triple, validate_target};

mod toolchain;
pub use toolchain::{ToolKind, ToolFamily, Tool, Toolchain, which};

pub mod supported_flags;
