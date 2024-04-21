use std::path::PathBuf;

mod apkanalyzer;

/// See <a href='https://developer.android.com/tools/apkanalyzer'>Android Apk Analyzer</a>
/// for more information.
#[derive(Debug, Clone)]
pub struct ApkAnalyzer {
	pub(crate) path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ApkSummary {
	pub package_name: String,
	pub version_code: String,
	pub version_name: String,
}
