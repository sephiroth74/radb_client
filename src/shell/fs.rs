//! Files on the device.

use std::ffi::OsString;
use std::fmt::{Display, Formatter};

use crate::error::{Error, Result};
use crate::shell::Shell;

/// Read / write / execute permissions.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ChmodMode {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

/// Owner / group / other permissions, e.g. `755`.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ChmodModeSpecs {
    pub owner: ChmodMode,
    pub group: ChmodMode,
    pub other: ChmodMode,
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ChmodOptions {
    pub mode: ChmodModeSpecs,
    /// `-R`: recurse into directories.
    pub recursive: bool,
    /// `-v`: verbose.
    pub verbose: bool,
    /// `-c`: print the changed files.
    pub changes: bool,
    /// `-f`: no error messages.
    pub quiet: bool,
}

impl Shell<'_> {
    /// The content of a file.
    pub fn cat<P: AsRef<str>>(&self, path: P) -> Result<Vec<u8>> {
        Ok(self.exec(["cat", path.as_ref()])?.stdout)
    }

    /// `ls [args] <path>`, one entry per line.
    pub fn ls<P: AsRef<str>>(&self, path: P, args: &[&str]) -> Result<Vec<String>> {
        let mut command = vec!["ls"];
        command.extend(args);
        command.push(path.as_ref());
        Ok(self
            .exec_string(command)?
            .lines()
            .map(str::to_string)
            .collect())
    }

    /// `rm [args] <path>`.
    pub fn rm<P: AsRef<str>>(&self, path: P, args: &[&str]) -> Result<()> {
        let mut command = vec!["rm"];
        command.extend(args);
        command.push(path.as_ref());
        self.exec(command).map(|_| ())
    }

    pub fn exists<P: AsRef<str>>(&self, path: P) -> Result<bool> {
        self.test_file(path, "e")
    }

    pub fn is_file<P: AsRef<str>>(&self, path: P) -> Result<bool> {
        self.test_file(path, "f")
    }

    pub fn is_dir<P: AsRef<str>>(&self, path: P) -> Result<bool> {
        self.test_file(path, "d")
    }

    pub fn is_symlink<P: AsRef<str>>(&self, path: P) -> Result<bool> {
        self.test_file(path, "h")
    }

    /// `test -<mode> <path>`, e.g. `mode = "x"` to check if the file is executable.
    pub fn test_file<P: AsRef<str>>(&self, path: P, mode: &str) -> Result<bool> {
        match self.exec(["test", &format!("-{mode}"), path.as_ref()]) {
            Ok(_) => Ok(true),
            Err(err) if err.exit_code() == Some(1) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// The disk usage (in KB) of each entry, as reported by `du [-s] <path>`.
    pub fn dir_size<P: AsRef<str>>(&self, path: P, summary: bool) -> Result<Vec<(String, u64)>> {
        let mut command = vec!["du"];
        if summary {
            command.push("-s");
        }
        command.push(path.as_ref());
        Ok(parse_du(&self.exec_string(command)?))
    }

    /// The permissions of a file (following symlinks).
    pub fn file_mode<P: AsRef<str>>(&self, path: P) -> Result<file_mode::Mode> {
        let output = self.exec_string(["stat", "-L", "-c", "%a", path.as_ref()])?;
        let mode = u32::from_str_radix(&output, 8)
            .map_err(|_| Error::ParseError(format!("file mode `{output}`")))?;
        Ok(file_mode::Mode::from(mode))
    }

    pub fn chmod<P: AsRef<str>>(&self, options: ChmodOptions, paths: &[P]) -> Result<()> {
        let mut command: Vec<OsString> = vec!["chmod".into()];
        command.extend(options);
        command.extend(paths.iter().map(|p| p.as_ref().into()));
        self.exec(command).map(|_| ())
    }
}

/// Parse `<size>\t<path>` lines (output of `du`).
fn parse_du(output: &str) -> Vec<(String, u64)> {
    output
        .lines()
        .filter_map(|line| {
            let (size, path) = line.split_once('\t')?;
            Some((path.to_string(), size.trim().parse().ok()?))
        })
        .collect()
}

impl ChmodMode {
    /// The octal digit of this mode.
    pub fn as_octal(&self) -> u32 {
        (self.read as u32) << 2 | (self.write as u32) << 1 | self.execute as u32
    }
}

impl From<ChmodMode> for i32 {
    fn from(value: ChmodMode) -> Self {
        value.as_octal() as i32
    }
}

impl Display for ChmodModeSpecs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.owner.as_octal(),
            self.group.as_octal(),
            self.other.as_octal()
        )
    }
}

impl IntoIterator for ChmodOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut args: Vec<OsString> = vec![];
        if self.recursive {
            args.push("-R".into());
        }
        if self.verbose {
            args.push("-v".into());
        }
        if self.changes {
            args.push("-c".into());
        }
        if self.quiet {
            args.push("-f".into());
        }
        args.push(self.mode.to_string().into());
        args.into_iter()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_du() {
        let entries = parse_du(include_str!("../../test_files/fixtures/api35/du.txt"));
        assert!(entries.contains(&("/data/local/tmp".to_string(), 672)));
        assert!(entries.contains(&("/sdcard/Download".to_string(), 8)));
        assert!(parse_du("garbage\nx\ty").is_empty());
    }

    #[test]
    fn test_chmod_options() {
        let rwx = ChmodMode {
            read: true,
            write: true,
            execute: true,
        };
        let rx = ChmodMode {
            read: true,
            write: false,
            execute: true,
        };
        let options = ChmodOptions {
            mode: ChmodModeSpecs {
                owner: rwx,
                group: rx,
                other: rx,
            },
            recursive: true,
            verbose: true,
            ..Default::default()
        };
        let args: Vec<String> = options
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        assert_eq!(args, ["-R", "-v", "755"]);
        assert_eq!(i32::from(rx), 5);
        assert_eq!(ChmodModeSpecs::default().to_string(), "000");
    }
}
