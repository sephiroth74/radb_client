//! Internal helpers to build and run commands.

use std::borrow::Cow;
use std::ffi::OsStr;
use std::process::Output;

use simple_cmd::debug::CommandDebug;
use simple_cmd::prelude::OutputExt;
use simple_cmd::CommandBuilder;

use crate::error::{CommandFailure, Result};

/// Quote `value` so that the device shell (`sh`) reads it as one literal word.
///
/// `adb shell` joins its arguments with spaces and the device shell parses the
/// resulting line again. Any argument with spaces, quotes or shell
/// metacharacters must be quoted, or the device sees something else.
/// Values made only of safe characters are returned unchanged.
///
/// ```
/// use radb_client::utils::shell_quote;
///
/// assert_eq!(shell_quote("com.example/.Main"), "com.example/.Main");
/// assert_eq!(shell_quote("a b"), "'a b'");
/// assert_eq!(shell_quote("it's"), r"'it'\''s'");
/// assert_eq!(shell_quote("$HOME"), "'$HOME'");
/// assert_eq!(shell_quote(""), "''");
/// ```
pub fn shell_quote(value: &str) -> Cow<'_, str> {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || "-_./=:,@%+".contains(c);
    if !value.is_empty() && value.chars().all(is_safe) {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(format!("'{}'", value.replace('\'', r"'\''")))
    }
}

/// Quote every argument for the device shell.
pub(crate) fn quote_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .map(|arg| shell_quote(&arg.as_ref().to_string_lossy()).into_owned())
        .collect()
}

/// Run the command. Fails with [`crate::Error::CommandFailed`] if it exits with a
/// non-zero status.
///
/// A process stopped by its cancel signal or its timeout is not a failure: the
/// output collected so far is returned (see `simple_cmd::prelude::OutputExt::kill`).
pub(crate) fn run(builder: CommandBuilder) -> Result<Output> {
    let cmd = builder.build();
    let command = cmd.as_string();
    let output = cmd.output()?;
    if output.success() || output.kill() || output.interrupt() {
        Ok(output)
    } else {
        Err(CommandFailure::new(command, &output).into())
    }
}

/// Run the command and return its output, whatever its exit status.
pub(crate) fn run_unchecked(builder: CommandBuilder) -> Result<Output> {
    Ok(builder.build().output()?)
}

/// Run the command and return its stdout as a trimmed string.
pub(crate) fn run_string(builder: CommandBuilder) -> Result<String> {
    let output = run(builder)?;
    stdout_string(&output)
}

pub(crate) fn stdout_string(output: &Output) -> Result<String> {
    Ok(std::str::from_utf8(&output.stdout)?.trim().to_string())
}

/// True if the process was stopped by a timeout or a cancel signal.
pub(crate) fn was_stopped(output: &Output) -> bool {
    output.kill() || output.interrupt()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_shell_quote() {
        assert_eq!(
            shell_quote("abc-DEF_1.0/x=y:z,w@v%u+t"),
            "abc-DEF_1.0/x=y:z,w@v%u+t"
        );
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("a;rm -rf /"), "'a;rm -rf /'");
        assert_eq!(shell_quote("`id`"), "'`id`'");
        assert_eq!(shell_quote("*"), "'*'");
        assert_eq!(shell_quote("~"), "'~'");
        assert_eq!(shell_quote("città"), "'città'");
    }

    #[test]
    fn test_quote_args() {
        assert_eq!(
            quote_args(["settings", "put", "global", "key", "a value"]),
            vec!["settings", "put", "global", "key", "'a value'"]
        );
    }
}
