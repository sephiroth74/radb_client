//! Read an APK on the host: the binary `AndroidManifest.xml` and the native libraries.

use std::path::Path;

use crate::error::{Error, Result};

const ATTR_NAME: u32 = 0x0101_0003;
const ATTR_VERSION_CODE: u32 = 0x0101_021b;
const ATTR_VERSION_NAME: u32 = 0x0101_021c;
const ATTR_SHARED_USER_ID: u32 = 0x0101_000b;
const ATTR_VERSION_CODE_MAJOR: u32 = 0x0101_0576;

/// What the manifest of an APK declares.
///
/// ```no_run
/// use radb_client::ApkManifest;
///
/// let manifest = ApkManifest::read("app-release.apk").unwrap();
/// println!("{} {}", manifest.package, manifest.version_code);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApkManifest {
    /// The package name (`package`).
    pub package: String,
    /// `android:versionCode`, with `android:versionCodeMajor` in the high 32 bits.
    pub version_code: i64,
    /// `android:versionName`.
    pub version_name: Option<String>,
    /// `android:sharedUserId`. The apps of a shared user must be signed with the same key.
    pub shared_user_id: Option<String>,
    /// `uses-permission` names.
    pub permissions: Vec<String>,
    /// ABIs of the native libraries (`lib/<abi>/*.so`).
    pub native_abis: Vec<String>,
}

/// A native library of an APK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeLibrary {
    pub abi: String,
    pub file_name: String,
    pub data: Vec<u8>,
}

impl ApkManifest {
    /// Read the manifest of an APK file on the host. No Android SDK is needed.
    pub fn read<P: AsRef<Path>>(apk: P) -> Result<ApkManifest> {
        let zip = std::fs::read(apk.as_ref())?;
        let entries = zip_entries(&zip)?;
        let manifest = entries
            .iter()
            .find(|e| e.name == "AndroidManifest.xml")
            .ok_or_else(|| {
                Error::ParseError(format!(
                    "{}: no AndroidManifest.xml",
                    apk.as_ref().display()
                ))
            })?;
        let mut result = parse_manifest(&zip_read(&zip, manifest)?)?;
        result.native_abis = native_entries(&entries)
            .map(|(abi, _)| abi.to_string())
            .collect();
        result.native_abis.dedup();
        Ok(result)
    }
}

/// The native libraries of an APK, for the given ABIs.
pub(crate) fn native_libraries<P: AsRef<Path>>(
    apk: P,
    abis: &[String],
) -> Result<Vec<NativeLibrary>> {
    let zip = std::fs::read(apk.as_ref())?;
    let entries = zip_entries(&zip)?;
    native_entries(&entries)
        .filter(|(abi, _)| abis.iter().any(|a| a == abi))
        .map(|(abi, entry)| {
            Ok(NativeLibrary {
                abi: abi.to_string(),
                file_name: entry
                    .name
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                data: zip_read(&zip, entry)?,
            })
        })
        .collect()
}

fn native_entries(entries: &[ZipEntry]) -> impl Iterator<Item = (&str, &ZipEntry)> {
    entries.iter().filter_map(|entry| {
        let rest = entry.name.strip_prefix("lib/")?;
        let (abi, file) = rest.split_once('/')?;
        (file.ends_with(".so") && !file.contains('/')).then_some((abi, entry))
    })
}

// region zip

#[derive(Debug, Clone)]
struct ZipEntry {
    name: String,
    method: u16,
    compressed_size: usize,
    local_header_offset: usize,
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    data.get(offset..offset + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| Error::ParseError("truncated APK".to_string()))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| Error::ParseError("truncated APK".to_string()))
}

/// The entries of the zip central directory.
///
/// An APK is a zip file. The end of central directory record (`PK\x05\x06`) is at the
/// end of the file, followed by an optional comment of up to 64 KB; it gives the number of
/// entries (offset 10) and the offset of the central directory (offset 16). Each central
/// directory entry (`PK\x01\x02`, 46 bytes + name + extra + comment) gives the compression
/// method (offset 10), the compressed size (20), the name length (28) and the offset of the
/// local header (42). Zip64 is not supported: APKs are smaller than 4 GB.
fn zip_entries(zip: &[u8]) -> Result<Vec<ZipEntry>> {
    const EOCD: u32 = 0x0605_4b50;
    const CENTRAL: u32 = 0x0201_4b50;
    // the end of central directory record is in the last 64 KB (it ends with a comment)
    let min = zip.len().saturating_sub(22 + 0xffff);
    let eocd = (min..=zip.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(zip, i).ok() == Some(EOCD))
        .ok_or_else(|| Error::ParseError("not a zip file".to_string()))?;
    let count = u16_at(zip, eocd + 10)? as usize;
    let mut offset = u32_at(zip, eocd + 16)? as usize;

    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(zip, offset)? != CENTRAL {
            return Err(Error::ParseError(
                "corrupted zip central directory".to_string(),
            ));
        }
        let name_len = u16_at(zip, offset + 28)? as usize;
        let extra_len = u16_at(zip, offset + 30)? as usize;
        let comment_len = u16_at(zip, offset + 32)? as usize;
        let name = zip
            .get(offset + 46..offset + 46 + name_len)
            .ok_or_else(|| Error::ParseError("truncated APK".to_string()))?;
        entries.push(ZipEntry {
            name: String::from_utf8_lossy(name).into_owned(),
            method: u16_at(zip, offset + 10)?,
            compressed_size: u32_at(zip, offset + 20)? as usize,
            local_header_offset: u32_at(zip, offset + 42)? as usize,
        });
        offset += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

/// The uncompressed data of an entry. The data follows the local header (`PK\x03\x04`,
/// 30 bytes + name + extra): its name and extra lengths (offsets 26 and 28) can differ from
/// the central directory ones. Entries are stored (0) or deflated (8).
fn zip_read(zip: &[u8], entry: &ZipEntry) -> Result<Vec<u8>> {
    let header = entry.local_header_offset;
    let start =
        header + 30 + u16_at(zip, header + 26)? as usize + u16_at(zip, header + 28)? as usize;
    let data = zip
        .get(start..start + entry.compressed_size)
        .ok_or_else(|| Error::ParseError(format!("truncated APK entry {}", entry.name)))?;
    match entry.method {
        0 => Ok(data.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec(data)
            .map_err(|e| Error::ParseError(format!("APK entry {}: {e:?}", entry.name))),
        method => Err(Error::ParseError(format!(
            "APK entry {}: unsupported compression {method}",
            entry.name
        ))),
    }
}

// endregion zip

// region binary xml

const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const TYPE_STRING: u8 = 0x03;
const TYPE_INT_DEC: u8 = 0x10;
const TYPE_INT_HEX: u8 = 0x11;

struct StringPool {
    strings: Vec<String>,
}

impl StringPool {
    fn parse(chunk: &[u8]) -> Result<StringPool> {
        let count = u32_at(chunk, 8)? as usize;
        let utf8 = u32_at(chunk, 16)? & 0x100 != 0;
        let strings_start = u32_at(chunk, 20)? as usize;
        let mut strings = Vec::with_capacity(count);
        for i in 0..count {
            let offset = strings_start + u32_at(chunk, 28 + i * 4)? as usize;
            strings.push(if utf8 {
                // utf-16 length, then utf-8 length (1 or 2 bytes each), then the bytes
                let skip_len = |at: usize| -> Result<(usize, usize)> {
                    let first = *chunk
                        .get(at)
                        .ok_or_else(|| Error::ParseError("truncated manifest".into()))?
                        as usize;
                    if first & 0x80 != 0 {
                        let second = *chunk
                            .get(at + 1)
                            .ok_or_else(|| Error::ParseError("truncated manifest".into()))?
                            as usize;
                        Ok((((first & 0x7f) << 8) | second, 2))
                    } else {
                        Ok((first, 1))
                    }
                };
                let (_, n1) = skip_len(offset)?;
                let (len, n2) = skip_len(offset + n1)?;
                let start = offset + n1 + n2;
                let bytes = chunk
                    .get(start..start + len)
                    .ok_or_else(|| Error::ParseError("truncated manifest".into()))?;
                String::from_utf8_lossy(bytes).into_owned()
            } else {
                let mut len = u16_at(chunk, offset)? as usize;
                let mut start = offset + 2;
                if len & 0x8000 != 0 {
                    len = ((len & 0x7fff) << 16) | u16_at(chunk, offset + 2)? as usize;
                    start += 2;
                }
                let units = (0..len)
                    .map(|i| u16_at(chunk, start + i * 2))
                    .collect::<Result<Vec<u16>>>()?;
                String::from_utf16_lossy(&units)
            });
        }
        Ok(StringPool { strings })
    }

    fn get(&self, index: u32) -> Option<&str> {
        self.strings.get(index as usize).map(String::as_str)
    }
}

/// Parse a binary (compiled) `AndroidManifest.xml`.
///
/// The file is a sequence of chunks, each starting with a header: type (u16), header size
/// (u16), total size (u32). After the `RES_XML_TYPE` file header come:
/// - the string pool: every name and string value is an index into it;
/// - the resource map: the resource id of the attribute names (`android:versionCode` is
///   `0x0101021b`), more reliable than the names, which shrinkers can strip;
/// - the XML nodes. A start element (`0x0102`) has, after its 16 bytes header, the
///   namespace and name indexes, then the attributes (20 bytes each): namespace, name, raw
///   string value (`0xffffffff` if none) and a typed value (size, type, data).
fn parse_manifest(xml: &[u8]) -> Result<ApkManifest> {
    let mut pool: Option<StringPool> = None;
    let mut resource_ids: Vec<u32> = vec![];
    let mut manifest = ApkManifest::default();
    let mut version_code_major: i64 = 0;

    let mut offset = u16_at(xml, 2)? as usize; // skip the RES_XML_TYPE header
    while offset + 8 <= xml.len() {
        let chunk_type = u16_at(xml, offset)?;
        let size = u32_at(xml, offset + 4)? as usize;
        if size < 8 || offset + size > xml.len() {
            return Err(Error::ParseError("corrupted binary manifest".to_string()));
        }
        let chunk = &xml[offset..offset + size];
        match chunk_type {
            RES_STRING_POOL_TYPE => pool = Some(StringPool::parse(chunk)?),
            RES_XML_RESOURCE_MAP_TYPE => {
                let header = u16_at(chunk, 2)? as usize;
                resource_ids = (header..size)
                    .step_by(4)
                    .map(|i| u32_at(chunk, i))
                    .collect::<Result<_>>()?;
            }
            RES_XML_START_ELEMENT_TYPE => {
                let pool = pool
                    .as_ref()
                    .ok_or_else(|| Error::ParseError("manifest without string pool".to_string()))?;
                let ext = u16_at(chunk, 2)? as usize;
                let element = pool.get(u32_at(chunk, ext + 4)?).unwrap_or_default();
                let attr_start = ext + u16_at(chunk, ext + 8)? as usize;
                let attr_size = u16_at(chunk, ext + 10)? as usize;
                let attr_count = u16_at(chunk, ext + 12)? as usize;

                for i in 0..attr_count {
                    let at = attr_start + i * attr_size;
                    let name_index = u32_at(chunk, at + 4)?;
                    let raw = u32_at(chunk, at + 8)?;
                    let data_type = *chunk
                        .get(at + 15)
                        .ok_or_else(|| Error::ParseError("truncated manifest".into()))?;
                    let data = u32_at(chunk, at + 16)?;
                    let id = resource_ids.get(name_index as usize).copied();
                    let name = pool.get(name_index).unwrap_or_default();

                    let string = || -> Option<String> {
                        if raw != u32::MAX {
                            pool.get(raw).map(str::to_string)
                        } else if data_type == TYPE_STRING {
                            pool.get(data).map(str::to_string)
                        } else {
                            None
                        }
                    };
                    let int = || -> Option<i64> {
                        match data_type {
                            TYPE_INT_DEC | TYPE_INT_HEX => Some(data as i64),
                            _ => string()?.parse().ok(),
                        }
                    };

                    match (element, id, name) {
                        ("manifest", _, "package") => {
                            manifest.package = string().unwrap_or_default()
                        }
                        ("manifest", Some(ATTR_VERSION_CODE), _) => {
                            manifest.version_code = int().unwrap_or_default()
                        }
                        ("manifest", Some(ATTR_VERSION_CODE_MAJOR), _) => {
                            version_code_major = int().unwrap_or_default()
                        }
                        ("manifest", Some(ATTR_VERSION_NAME), _) => {
                            manifest.version_name = string()
                        }
                        ("manifest", Some(ATTR_SHARED_USER_ID), _) => {
                            manifest.shared_user_id = string()
                        }
                        ("uses-permission", Some(ATTR_NAME), _) => {
                            manifest.permissions.extend(string())
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        offset += size;
    }

    if manifest.package.is_empty() {
        return Err(Error::ParseError("manifest without package".to_string()));
    }
    manifest.version_code |= version_code_major << 32;
    Ok(manifest)
}

// endregion binary xml

#[cfg(test)]
mod test {
    use super::*;

    fn test_file(name: &str) -> String {
        format!("{}/test_files/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn test_read_manifest() {
        let manifest = ApkManifest::read(test_file("app-debug.apk")).unwrap();
        assert_eq!(manifest.package, "it.sephiroth.android.app.app");
        assert_eq!(manifest.version_code, 1);
        assert_eq!(manifest.version_name.as_deref(), Some("1.0"));
        assert_eq!(manifest.shared_user_id, None);
        assert_eq!(
            manifest.permissions,
            ["it.sephiroth.android.app.app.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"]
        );
        assert!(manifest.native_abis.is_empty());
    }

    #[test]
    fn test_system_app_fixtures() {
        let read =
            |name: &str| ApkManifest::read(test_file(&format!("system_app/{name}.apk"))).unwrap();
        let v1 = read("sysapp-v1-debug");
        assert_eq!(v1.package, "it.sephiroth.radb.sysapp");
        assert_eq!(v1.version_code, 1);
        assert_eq!(v1.version_name.as_deref(), Some("1.0-debug"));
        assert!(v1.permissions.is_empty());

        assert_eq!(read("sysapp-v3-release").version_code, 3);
        assert_eq!(
            read("sysapp-v4-privileged").permissions,
            ["android.permission.INSTALL_PACKAGES"]
        );
        assert_eq!(
            read("sysapp-v5-native").native_abis,
            ["arm64-v8a", "x86_64", "armeabi-v7a"]
        );
    }

    #[test]
    fn test_native_libraries() {
        let apk = test_file("system_app/sysapp-v5-native.apk");
        let libs =
            native_libraries(&apk, &["x86_64".to_string(), "arm64-v8a".to_string()]).unwrap();
        assert_eq!(libs.len(), 2);
        assert!(libs
            .iter()
            .all(|l| l.file_name == "libradb.so" && l.data.len() == 4096));
        assert!(native_libraries(&apk, &["mips".to_string()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_not_an_apk() {
        assert!(ApkManifest::read(test_file("fixtures/adb_devices.txt")).is_err());
        assert!(ApkManifest::read("/nonexistent.apk").is_err());
    }
}
