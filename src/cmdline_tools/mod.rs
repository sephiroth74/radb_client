//! Wrappers of the Android SDK command-line tools.

use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use simple_cmd::CommandBuilder;

use crate::command::run;
use crate::error::{Error, Result};

/// `apkanalyzer`, from the SDK `cmdline-tools`.
/// See <https://developer.android.com/tools/apkanalyzer>.
#[derive(Debug, Clone)]
pub struct ApkAnalyzer {
    pub(crate) path: PathBuf,
    pub(crate) debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The summary of an APK (`apkanalyzer apk summary`).
pub struct ApkSummary {
    /// The package name.
    pub package_name: String,
    /// The version code.
    pub version_code: String,
    /// The version name.
    pub version_name: String,
}

static RE_CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^C\s+d\s[0-9\t]+(.*)").unwrap());

impl ApkAnalyzer {
    /// Find `apkanalyzer` in the Android SDK (see [`crate::utils::android_home`]).
    pub fn new() -> Result<Self> {
        Ok(ApkAnalyzer {
            path: crate::utils::apk_analyzer()?,
            debug: false,
        })
    }

    /// Use the `apkanalyzer` at `path`, logging the command lines if `debug`.
    pub fn from<P: AsRef<Path>>(path: P, debug: bool) -> Self {
        ApkAnalyzer {
            path: path.as_ref().to_path_buf(),
            debug,
        }
    }

    fn command(&self, args: &[&str]) -> CommandBuilder {
        CommandBuilder::new(&self.path)
            .with_debug(self.debug)
            .args(args)
    }

    /// Run `apkanalyzer <args> <apk>` and return its stdout.
    fn output<P: AsRef<Path>>(&self, args: &[&str], apk_path: P) -> Result<String> {
        let output = run(self.command(args).arg(apk_path.as_ref()))?;
        Ok(String::from_utf8(output.stdout)?)
    }

    fn trimmed<P: AsRef<Path>>(&self, args: &[&str], apk_path: P) -> Result<String> {
        Ok(self.output(args, apk_path)?.trim().to_string())
    }

    fn lines<P: AsRef<Path>>(&self, args: &[&str], apk_path: P) -> Result<Vec<String>> {
        Ok(self
            .output(args, apk_path)?
            .lines()
            .map(str::to_string)
            .collect())
    }

    /// `apkanalyzer manifest version-name`
    pub fn version_name<P: AsRef<Path>>(&self, apk_path: P) -> Result<String> {
        self.trimmed(&["manifest", "version-name"], apk_path)
    }

    /// `apkanalyzer manifest version-code`
    pub fn version_code<P: AsRef<Path>>(&self, apk_path: P) -> Result<String> {
        self.trimmed(&["manifest", "version-code"], apk_path)
    }

    /// `apkanalyzer manifest application-id`: the package name.
    pub fn application_id<P: AsRef<Path>>(&self, apk_path: P) -> Result<String> {
        self.trimmed(&["manifest", "application-id"], apk_path)
    }

    /// `apkanalyzer manifest min-sdk`
    pub fn min_sdk<P: AsRef<Path>>(&self, apk_path: P) -> Result<i64> {
        Ok(self.trimmed(&["manifest", "min-sdk"], apk_path)?.parse()?)
    }

    /// `apkanalyzer manifest target-sdk`
    pub fn target_sdk<P: AsRef<Path>>(&self, apk_path: P) -> Result<i64> {
        Ok(self
            .trimmed(&["manifest", "target-sdk"], apk_path)?
            .parse()?)
    }

    /// The decoded `AndroidManifest.xml`.
    pub fn manifest_code<P: AsRef<Path>>(&self, apk_path: P) -> Result<String> {
        self.trimmed(&["manifest", "print"], apk_path)
    }

    /// `apkanalyzer manifest debuggable`
    pub fn debuggable<P: AsRef<Path>>(&self, apk_path: P) -> Result<bool> {
        Ok(self.trimmed(&["manifest", "debuggable"], apk_path)? == "true")
    }

    /// The `uses-permission` of the manifest (`apkanalyzer manifest permissions`).
    pub fn manifest_permissions<P: AsRef<Path>>(&self, apk_path: P) -> Result<Vec<String>> {
        self.lines(&["manifest", "permissions"], apk_path)
    }

    /// The files of the APK (`apkanalyzer files list`).
    pub fn files_list<P: AsRef<Path>>(&self, apk_path: P) -> Result<Vec<PathBuf>> {
        Ok(self
            .lines(&["files", "list"], apk_path)?
            .into_iter()
            .map(PathBuf::from)
            .collect())
    }

    /// The content of a file of the APK, e.g. `/AndroidManifest.xml`.
    pub fn files_cat<A: AsRef<Path>, P: AsRef<str>>(&self, apk_path: A, path: P) -> Result<String> {
        self.trimmed(&["files", "cat", "--file", path.as_ref()], apk_path)
    }

    /// Package name, version code and version name (`apkanalyzer apk summary`).
    pub fn summary<P: AsRef<Path>>(&self, apk_path: P) -> Result<ApkSummary> {
        let output = self.trimmed(&["apk", "summary"], apk_path)?;
        let mut parts = output.splitn(3, char::is_whitespace).map(str::trim);
        match (parts.next(), parts.next(), parts.next()) {
            (Some(package_name), Some(version_code), Some(version_name)) => Ok(ApkSummary {
                package_name: package_name.to_string(),
                version_code: version_code.to_string(),
                version_name: version_name.to_string(),
            }),
            _ => Err(Error::ParseError(format!("apk summary `{output}`"))),
        }
    }

    /// The dex files of the APK (`apkanalyzer dex list`).
    pub fn dex_list<P: AsRef<Path>>(&self, apk_path: P) -> Result<Vec<String>> {
        self.lines(&["dex", "list"], apk_path)
    }

    /// The bytecode of a class.
    pub fn dex_code<P: AsRef<Path>>(&self, apk_path: P, class_name: &str) -> Result<String> {
        self.output(&["dex", "code", "--class", class_name], apk_path)
    }

    /// The classes of the APK.
    pub fn classes<P: AsRef<Path>>(
        &self,
        apk_path: P,
        dex_names: Option<Vec<&str>>,
        defined_only: Option<bool>,
    ) -> Result<Vec<String>> {
        let mut args = vec!["dex", "packages"];
        if defined_only == Some(true) {
            args.push("--defined-only");
        }
        for dex_name in dex_names.unwrap_or_default() {
            args.extend(["--files", dex_name]);
        }
        Ok(parse_classes(&self.output(&args, apk_path)?))
    }
}

/// The class names (`C d ...` lines) of `apkanalyzer dex packages`.
fn parse_classes(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| RE_CLASS.captures(line).map(|c| c[1].to_string()))
        .collect()
}

impl Display for ApkSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "package_name: {}, version_code: {}, version_name: {}",
            self.package_name, self.version_code, self.version_name
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const APK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_files/app-debug.apk");

    /// `None` (the test is skipped) if the SDK cmdline-tools are not installed.
    fn analyzer() -> Option<ApkAnalyzer> {
        match ApkAnalyzer::new() {
            Ok(analyzer) => Some(analyzer),
            Err(err) => {
                eprintln!("skipped: {err}");
                None
            }
        }
    }

    #[test]
    fn test_parse_classes() {
        let output = "P d 1\t2\t3\tit.sephiroth\nC d 1\t2\t3\tit.sephiroth.MainActivity\nM d 1\t1\tvoid onCreate()\n";
        assert_eq!(parse_classes(output), ["it.sephiroth.MainActivity"]);
    }

    #[test]
    fn test_manifest() {
        let Some(analyzer) = analyzer() else { return };
        assert_eq!(
            analyzer.application_id(APK).unwrap(),
            "it.sephiroth.android.app.app"
        );
        assert_eq!(analyzer.version_name(APK).unwrap(), "1.0");
        assert_eq!(analyzer.version_code(APK).unwrap(), "1");
        assert_eq!(analyzer.min_sdk(APK).unwrap(), 24);
        assert!(analyzer.target_sdk(APK).unwrap() >= 24);
        assert!(analyzer.debuggable(APK).unwrap());
        assert!(analyzer.manifest_code(APK).unwrap().contains("<manifest"));
        assert_eq!(
            analyzer.manifest_permissions(APK).unwrap(),
            ["it.sephiroth.android.app.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"]
        );
    }

    #[test]
    fn test_summary_and_files() {
        let Some(analyzer) = analyzer() else { return };
        assert_eq!(
            analyzer.summary(APK).unwrap(),
            ApkSummary {
                package_name: "it.sephiroth.android.app.app".to_string(),
                version_code: "1".to_string(),
                version_name: "1.0".to_string(),
            }
        );
        let files = analyzer.files_list(APK).unwrap();
        assert!(files.contains(&PathBuf::from("/AndroidManifest.xml")));
        assert!(!analyzer.dex_list(APK).unwrap().is_empty());
        assert!(analyzer
            .classes(APK, None, Some(true))
            .unwrap()
            .iter()
            .any(|c| c.ends_with("MainActivity")));
    }

    #[test]
    fn test_missing_apk_is_an_error() {
        let Some(analyzer) = analyzer() else { return };
        assert!(analyzer.version_name("/nonexistent.apk").is_err());
    }
}
