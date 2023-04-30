use std::io::{self, Write};
use std::path::{Path, PathBuf};


// Parse dependency file `.d` and return list of dependencies
pub fn read_dependency_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<PathBuf>> {
    let data = std::fs::read_to_string(path.as_ref())?;
    if data.starts_with(HEADER) {
        Ok(data.lines().skip(1).map(PathBuf::from).collect())
    } else {
        parse_dependency_file_unix(&data)
    }
}


// Write dependencies to file
pub fn write_dependency_file(path: &Path, includes: &Vec<PathBuf>) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write!(file, "{}\n", HEADER)?;
    for (i, v) in includes.iter().enumerate() {
        if i > 0 { write!(file, "\n")?; }
        write!(file, "{}", v.display())?;
    }
    Ok(())
}


// Parse unix dependency file `.d`
fn parse_dependency_file_unix(deps: &str) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut i = 0;
    for line in deps.split(' ') {
        let line = line.trim().trim_matches('\\');
        if !line.is_empty() {
            // we are only interested in dependencies, so the first two paths are ignored
            // - first is the output object file path
            // - second is the input source file path
            if i > 1 { 
                paths.push(PathBuf::from(line));
            }
            i += 1;
        }
    }
    Ok(paths)
}


// Header for identifying windows dependencies
const HEADER: &str = "@DEPS@";
