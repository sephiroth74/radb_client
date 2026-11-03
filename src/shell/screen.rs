//! Screenshots, screen recording and mirroring.

use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use crossbeam_channel::Receiver;
use simple_cmd::debug::CommandDebug;

use crate::command::quote_args;
use crate::error::Result;
use crate::shell::Shell;

/// Options of `screenrecord`.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ScreenRecordOptions {
    /// `--bit-rate`: bits per second (default 20Mbps).
    pub bitrate: Option<u64>,
    /// `--time-limit`: maximum recording time (default and maximum: 180 seconds).
    pub timelimit: Option<Duration>,
    /// `--rotate`: rotate the output 90 degrees (experimental).
    pub rotate: Option<bool>,
    /// `--bugreport`: add information, such as a timestamp overlay.
    pub bug_report: Option<bool>,
    /// `--size WIDTHxHEIGHT`.
    pub size: Option<(u16, u16)>,
    /// `--verbose`.
    pub verbose: bool,
}

/// Options of `ffplay`, for [`Shell::screen_mirror`].
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FFPlayOptions {
    pub framerate: Option<u16>,
    pub size: Option<(u16, u16)>,
    pub probesize: Option<u16>,
}

impl Shell<'_> {
    /// Save a PNG screenshot to a file on the device.
    pub fn save_screencap<P: AsRef<str>>(&self, path: P) -> Result<()> {
        self.exec(["screencap", "-p", path.as_ref()]).map(|_| ())
    }

    /// Record the screen to a file on the device, until the time limit or
    /// until `cancel` receives a message.
    pub fn screen_record<P: AsRef<str>>(
        &self,
        options: Option<ScreenRecordOptions>,
        output: P,
        cancel: Option<Receiver<()>>,
    ) -> Result<()> {
        let mut args: Vec<OsString> = vec!["screenrecord".into()];
        if let Some(options) = options {
            args.extend(options);
        }
        args.push(output.as_ref().into());
        self.exec_with(args, cancel, None).map(|_| ())
    }

    /// Mirror the screen in a `ffplay` window on the host (`ffplay` must be in `PATH`).
    pub fn screen_mirror(
        &self,
        screenrecord_options: ScreenRecordOptions,
        play_options: FFPlayOptions,
        cancel: Option<Receiver<()>>,
    ) -> Result<()> {
        let mut args: Vec<OsString> = vec!["screenrecord".into(), "--output-format=h264".into()];
        args.extend(screenrecord_options);
        args.push("-".into());

        let record = self
            .parent
            .command()
            .arg("shell")
            .args(quote_args(args))
            .signal(cancel)
            .build();

        let mut play = std::process::Command::new(which::which("ffplay")?);
        play.args(play_options).args([
            "-loglevel",
            "repeat+level+verbose",
            "-an",
            "-autoexit",
            "-sync",
            "video",
            "-",
        ]);
        if self.parent.debug {
            play.debug();
        }
        record.pipe(play)?;
        Ok(())
    }
}

impl ScreenRecordOptions {
    pub fn new() -> Self {
        ScreenRecordOptions {
            bitrate: Some(4_000_000),
            timelimit: Some(Duration::from_secs(10)),
            rotate: None,
            bug_report: None,
            size: None,
            verbose: false,
        }
    }
}

impl Default for ScreenRecordOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for ScreenRecordOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut args: Vec<OsString> = vec![];
        if let Some(bitrate) = self.bitrate {
            args.extend(["--bit-rate".into(), bitrate.to_string().into()]);
        }
        if let Some(timelimit) = self.timelimit {
            args.extend([
                "--time-limit".into(),
                timelimit.as_secs().to_string().into(),
            ]);
        }
        if self.rotate.unwrap_or(false) {
            args.push("--rotate".into())
        }
        if self.bug_report.unwrap_or(false) {
            args.push("--bugreport".into())
        }
        if self.verbose {
            args.push("--verbose".into())
        }
        if let Some((width, height)) = self.size {
            args.extend(["--size".into(), format!("{width}x{height}").into()]);
        }
        args.into_iter()
    }
}

impl Display for ScreenRecordOptions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let args: Vec<String> = self
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        write!(f, "{}", args.join(" "))
    }
}

impl Default for FFPlayOptions {
    fn default() -> Self {
        FFPlayOptions {
            framerate: Some(30),
            size: Some((1440, 800)),
            probesize: Some(300),
        }
    }
}

impl IntoIterator for FFPlayOptions {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let mut args: Vec<OsString> = vec![];
        if let Some(framerate) = self.framerate {
            args.extend(["-framerate".into(), framerate.to_string().into()]);
        }
        if let Some(probesize) = self.probesize {
            args.extend(["-probesize".into(), probesize.to_string().into()]);
        }
        if let Some((width, height)) = self.size {
            args.extend(["-vf".into(), format!("scale={width}:{height}").into()]);
        }
        args.into_iter()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_screenrecord_options() {
        assert_eq!(
            ScreenRecordOptions::new().to_string(),
            "--bit-rate 4000000 --time-limit 10"
        );
        let options = ScreenRecordOptions {
            bitrate: None,
            timelimit: None,
            rotate: Some(true),
            bug_report: Some(true),
            size: Some((1280, 720)),
            verbose: true,
        };
        assert_eq!(
            options.to_string(),
            "--rotate --bugreport --verbose --size 1280x720"
        );
    }

    #[test]
    fn test_ffplay_options() {
        let args: Vec<String> = FFPlayOptions::default()
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        assert_eq!(
            args,
            [
                "-framerate",
                "30",
                "-probesize",
                "300",
                "-vf",
                "scale=1440:800"
            ]
        );
    }
}
