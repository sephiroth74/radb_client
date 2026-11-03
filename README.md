# radb_client

Android **adb** client for Rust. It runs the `adb` executable found in your `PATH` and
parses its output: shell commands, packages, activities, properties, settings, files,
input events, logcat, screenshots, system apps...

[![crates.io](https://img.shields.io/crates/v/radb_client.svg)](https://crates.io/crates/radb_client/)

- Upgrading from 1.x? The [CHANGELOG](CHANGELOG.md) lists what changed and why.
- API documentation: `cargo doc --open`, or [docs.rs](https://docs.rs/radb_client).
- Hosts: macOS and Linux (`adb` from the Android SDK platform-tools).

## Example

```rust,no_run
use radb_client::{Adb, Client, ConnectionType, Intent, SettingsType};

fn main() -> radb_client::Result<()> {
    // a device on the network (the port defaults to 5555)...
    let client = Client::try_from(ConnectionType::try_from_ip("192.168.1.128")?)?;
    client.connect(None)?;

    // ...or any device by serial: emulator-5554, a USB serial...
    let emulator = Client::new(Adb::new()?, ConnectionType::emulator(5554), false);

    let shell = emulator.shell();
    println!("API {}", shell.build_version_sdk()?);
    shell.put_setting(SettingsType::global, "my_key", "a value with spaces")?;

    let mut intent = Intent::from_action("android.intent.action.VIEW");
    intent.data = Some("https://example.com".to_string());
    emulator.am().start(&intent)?;
    Ok(())
}
```

## Errors

Every command that exits with a non-zero status fails with `Error::CommandFailed`: the
command line, the exit code, stdout and stderr. A failure is never an `Ok` with an empty
output.

```rust,no_run
# use radb_client::{Client, ConnectionType, Error};
# let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
match client.shell().cat("/system/build.prop") {
    Ok(content) => println!("{} bytes", content.len()),
    // `adb -s emulator-5554 shell cat /system/build.prop` failed with exit code 1:
    // cat: /system/build.prop: Permission denied
    Err(Error::CommandFailed(failure)) => eprintln!("{failure}"),
    Err(err) => eprintln!("{err}"),
}
```

## Shell commands

`adb shell` joins its arguments with spaces and the device shell parses them again, so an
argument with a space, a quote or a `$` arrives broken. `Shell::exec` quotes each argument:
it reaches the device program unchanged.

```rust,no_run
# use radb_client::{Client, ConnectionType};
# let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
let shell = client.shell();
shell.exec(["touch", "/sdcard/Download/my file.txt"])?;  // one file, not two
shell.exec_script("ls /system/app | wc -l")?;            // a script: pipes, variables...
# Ok::<(), radb_client::Error>(())
```

- `Shell::exec_with`: the same, with a timeout and a cancel signal.
- `Shell::exec_script`: a shell script; quote the values you interpolate with
  `radb_client::utils::shell_quote`.
- `Shell::exec_raw`: the plain `adb shell` behavior (no quoting, no exit status check).
- `Shell::spawn`: start a command without waiting for it.

## Devices

`ConnectionType` selects the device. Prefer a serial (`ConnectionType::Serial`, or
`ConnectionType::emulator(5554)`) or an address (`ConnectionType::TcpIp`): a transport id
(`ConnectionType::Transport`) changes every time the connection is re-established, e.g.
after `adb root` or a reboot.

`Adb::list_devices` returns the devices in any state (online, offline, unauthorized,
recovery...), each with the `ConnectionType` to use.

## System apps

Installing a new version of a system app (in `/system/priv-app` or `/system/app`) usually
goes wrong in one of these ways:

- `adb install` never touches `/system`: it installs an *update* in `/data/app`, and only
  when the APK is signed with the same key as the system copy. A debug build can't replace a
  release build this way.
- An update with a version lower than the system copy is accepted by `adb install -d`, and
  silently deleted by the package manager at the next boot.
- `pm uninstall --user 0` does not uninstall a system app: it hides it for user 0.
- Replacing the APK in `/system` by hand needs root, a remount, the removal of the update in
  `/data/app`, of the old copy (maybe an APK placed directly in `priv-app/`, maybe several
  copies), of its compiled code, the right permissions and SELinux labels, the native
  libraries extracted (the package manager doesn't extract them for system apps), a stale
  package manager cache invalidated, and a reboot.
- A `priv-app` requesting a privileged permission that the `privapp-permissions` allowlist
  does not grant makes the device boot loop (`ro.control_privapp_permissions=enforce`).

`Client::install_system_app` handles all of that, whatever the state of the package:

```rust,no_run
use radb_client::{Client, ConnectionType, SystemAppInstallOptions, SystemAppStrategy};

fn main() -> radb_client::Result<()> {
    let client = Client::try_from(ConnectionType::try_from_ip("192.168.1.114")?)?;
    client.connect(None)?;

    // an update when possible, otherwise the system copy is replaced (with a reboot)
    let report = client.install_system_app("app-debug.apk", SystemAppInstallOptions::default())?;
    println!("{:?}: {}", report.method, report.after.active.code_path);

    // always replace the system copy, and clear the app data
    let options = SystemAppInstallOptions {
        strategy: SystemAppStrategy::ReplaceSystemCopy,
        wipe_data: true,
        ..Default::default()
    };
    client.install_system_app("app-release.apk", options)?;

    // where are the copies of a package?
    let info = client.pm().system_app_info("com.example.app")?;
    println!("system copy: {:?}, update: {:?}", info.system_copy, info.update);
    Ok(())
}
```

- **Strategy**: `Auto` installs an update (`adb install -r`, no reboot) when the package is
  a system app signed with the same key and the new version is not lower than the system
  copy; otherwise it replaces the system copy. `ReplaceSystemCopy` always replaces it.
- **Privileged permissions** of the platform are checked *before* the device is changed: by
  default the installation fails with `Error::PrivappPermissionsError`;
  `PrivappPermissionsPolicy::Allowlist` writes an allowlist for them.
- **Rollback**: if the device does not boot, or does not run the new copy, the previous
  system copy is restored.
- **Data**: the package manager keeps the app data, also when the signature changes or the
  version is lower: use `wipe_data` to clear it.
- **Duplicates**: if after the reboot the package manager runs another system copy of the
  package, it is removed too.

`Client::remove_system_app` removes a system app completely: its updates, all its system
copies, its data. `PackageManager::uninstall_system_updates` only removes the updates.

Replacing the system copy needs a `userdebug` or `eng` build (`adb root`) and a remountable
system partition (`adb remount`, overlayfs on recent devices). The example
`examples/system_app.rs` is a command line tool for it:

```sh
cargo run --example system_app -- 192.168.1.114 info com.example.app
cargo run --example system_app -- 192.168.1.114 install app-release.apk --force --wipe-data
cargo run --example system_app -- emulator-5554 remove com.example.app
```

## APKs

`ApkManifest::read` reads the package name, version, permissions and native ABIs of an APK
on the host, without the Android SDK. `cmdline_tools::ApkAnalyzer` wraps the SDK
`apkanalyzer` for everything else.

## Features

- `scanner`: scan a network range for adb devices (`radb_client::scanner::Scanner`).
- `clipboard`: `Client::copy_screencap`, copy a screenshot to the host clipboard.
- `emulator-tests`: builds the integration tests (see below).

```rust,ignore
use std::str::FromStr;
use cidr_utils::cidr::Ipv4Cidr;
use itertools::Either;
use radb_client::{scanner::Scanner, Adb};

let cidr = Ipv4Cidr::from_str("192.168.1.0/24").unwrap();
let (tx, rx) = crossbeam_channel::unbounded();
Scanner::default().scan(&Adb::new().unwrap(), cidr.iter(), tx);
for message in rx {
    match message {
        Either::Left(addr) => println!("scanning {addr}..."),
        Either::Right(device) => println!("found {device}"),
    }
}
```

## Tests

```sh
cargo test --features scanner,clipboard   # unit tests and doctests: no device needed
```

The unit tests parse outputs captured from Android 14 and 15 emulators
(`test_files/fixtures`), and the APKs in `test_files` (`test_files/system_app/build.sh`
builds the system app ones).

The integration tests in `tests/emulator` need a running emulator. They change its state
(root, settings, packages, system apps...), so they run one at a time, and never pick a
physical device unless you ask for it.

```sh
# once: an image with root (google_apis, not google_apis_playstore)
android sdk install system-images/android-35/google_apis/arm64-v8a   # x86_64 on Intel
avdmanager create avd -n radb-test -k "system-images;android-35;google_apis;arm64-v8a" -d pixel_6

emulator -avd radb-test -no-window -no-audio -no-snapshot -writable-system &
adb wait-for-device

cargo test --features emulator-tests,scanner --test emulator
# also the tests that reboot the emulator (system apps: many reboots)
cargo test --features emulator-tests,scanner --test emulator -- --include-ignored
```

The tests use the first running emulator, or the device in `RADB_TEST_SERIAL` (a physical
device also needs `RADB_TEST_ALLOW_PHYSICAL=1`). `-writable-system` is needed by the system
app tests.

The CI (`.github/workflows/ci.yml`) runs all of them on an API 35 emulator.
