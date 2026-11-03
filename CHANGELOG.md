# Changelog

Each entry says what changed, **why**, and how to migrate.

## 2.0.0

### Why a major release

The 1.x API had problems that could not be fixed without breaking it:

- **Failures looked like successes.** The exit status of most commands was ignored: a `cat`
  of a missing file, a failed `setprop` or an `am start` of an unknown activity returned
  `Ok`, with an empty or partial output. The errors surfaced later, far from their cause
  (e.g. `MacParseError` for a device that was offline).
- **Arguments were broken by the device shell.** `adb shell` joins its arguments with spaces
  and the device `sh` parses the line again: a value with a space was split in two, a quote
  broke the command (`no closing quote`), `$HOME` or a backtick were expanded. For example an
  intent with `-d 'https://example.com/a b'` was started with `pkg=b`.
- **Devices got lost.** Devices listed by `adb devices` were addressed by transport id, which
  changes every time adbd restarts, `adb root` included: a `Client` stopped working after
  `client.root()`.
- **The tests needed one specific physical device** on the author's network, and some of them
  changed its state (reboot, verity, a factory reset broadcast). `cargo test` also ran the
  doctests, which connected to that device.
- **System apps** could not be installed or replaced reliably (see *Added*).

### Migration guide

| 1.x | 2.0 |
|---|---|
| `shell.exec(["ls", "-l"], None, None)` | `shell.exec(["ls", "-l"])` (quoted, fails on a non-zero exit) |
| `shell.exec(vec!["ls -l /sdcard \| wc -l"], None, None)` | `shell.exec_script("ls -l /sdcard \| wc -l")` |
| `shell.exec(args, cancel, timeout)` | `shell.exec_with(args, cancel, timeout)` |
| the 1.x `exec` behavior (unquoted, unchecked) | `shell.exec_raw(args, cancel, timeout)` |
| `shell.try_send_keyevent(...)`, `try_exec(...)` | `shell.send_keyevent(...)`, or `shell.spawn(args)` |
| `adb.list_devices(true)` | `adb.with_debug(true).list_devices()` |
| `device.name`, `device.connected` | `device.serial`, `device.is_online()` / `device.state` |
| `Client::try_from(&device)` with a transport id | unchanged: devices are now addressed by serial |
| `let conn2 = conn;` (`ConnectionType: Copy`) | `let conn2 = conn.clone();` |
| `client.root()? == true` | `client.root()?` (`Result<()>`) |
| `client.get_seriano()` | `client.get_serialno()` |
| `client.disconnect()` of a USB device | an error: only tcp/ip devices can be disconnected |
| `radb_client::result::Result` | `radb_client::Result` |
| `anyhow::Result` from `ApkAnalyzer` and `utils` | `radb_client::Result` |
| `shell.dumpsys(Some("x"), None, None, false, false, false, None)` | `shell.dumpsys(Some("x"), &[], DumpsysOptions::default())` |
| `SimplePackageReader::new(data, sdk)` | `SimplePackageReader::new(data)` |
| `shell.which("x")` → `Option<String>` | `shell.which("x")?` → `Option<String>` |
| `radb_client::types::Client` | unchanged, and also `radb_client::Client` |

### Breaking changes

**Errors and exit codes**

- Commands that exit with a non-zero status fail with `Error::CommandFailed(CommandFailure)`,
  which carries the command line, the exit code, stdout and stderr
  (`Error::exit_code()` returns the exit code).
  *Why:* see "failures looked like successes" above. The command line and the output make the
  error actionable. Some methods turn a documented exit code into an answer: `which` returns
  `None`, `test_file` returns `false`, `pm path` returns `PackageNotFoundError`.
- A command stopped by its timeout or cancel signal is not an error: the output collected so
  far is returned. *Why:* `logcat`, `screenrecord` and `exec_with` are stopped on purpose.
- `Error` is `#[non_exhaustive]`. `ParseInputError` is replaced by `ParseError(String)`, which
  says what could not be parsed. New variants: `CommandFailed`, `Timeout`, `ToolNotFound`,
  `Utf8Error`, `SystemAppError`, `PrivappPermissionsError`.
  *Why:* new variants can be added without another major release; an error must say what failed.
- `utils` and `cmdline_tools::ApkAnalyzer` return `radb_client::Result` instead of
  `anyhow::Result`. *Why:* a library should expose one error type that callers can match.
- `radb_client::result::Result` is now `radb_client::Result` (also `radb_client::error::Result`).

**Shell**

- `Shell::exec(args)` quotes each argument (`utils::shell_quote`) and checks the exit status.
  The 1.x `exec(args, cancel, timeout)` maps to `exec_with` (quoted, checked), `exec_raw`
  (the 1.x behavior) or `exec_script` (a script with pipes, variables...).
  *Why:* see "arguments were broken by the device shell". The signature changed on purpose, so
  that 1.x calls that relied on the shell parsing (`exec(vec!["ls -l /sdcard"])`) fail to
  compile instead of silently running a different command.
- The `try_*` methods (`try_exec`, `try_send_keyevent`...) are removed; use `Shell::spawn`,
  which returns the `std::process::Child`.
  *Why:* they never waited for the command (`Ok(None)` almost always), panicked when the spawn
  failed, and left zombie processes.
- `Shell` is `Copy`, and `Shell::am()`/`pm()` return handles that borrow the client instead of
  the shell; there are also `Client::am()` and `Client::pm()`.
  *Why:* `let am = client.shell().am();` did not compile (the temporary shell was dropped).
- Device paths are `impl AsRef<str>` instead of `rustix::path::Arg`; local paths are
  `impl AsRef<Path>`. *Why:* `rustix` was a public dependency only for this trait.
- `which()` returns `Result<Option<String>>`; `get_command_path()` and its global cache are
  removed. *Why:* errors were hidden in `None`, and the cache could go stale (remount, reboot).
- `ls(path, &[args])` and `rm(path, &[args])` take slices.
- `dumpsys(service, args, DumpsysOptions)` replaces 7 positional arguments.
- `chmod`, `save_screencap`, `screen_record`, `screen_mirror`, `Client::pull`, `Client::push`,
  `Client::bug_report` return `Result<()>`; `Client::logcat` returns the log text.
  *Why:* actions return `Result<()>`, queries return values; the raw `Output` is only returned by
  the `exec*` methods.
- `build_version_release()` returns a `String`. *Why:* releases like `8.1.0` are not integers.
- `send_text` replaces the spaces with `%s`, which `input text` reads as a space.

**Devices and connections**

- `ConnectionType` has new variants, `Serial(String)` (`-s serial`) and `Emulator` (`-e`); it
  is `Clone` instead of `Copy`, `#[non_exhaustive]`, and `Transport` takes a `u64`.
  *Why:* a serial survives adbd restarts (`emulator-5554` stays `emulator-5554` after
  `adb root`, its transport id does not); `u8` transport ids overflowed on long-running adb
  servers.
- `ConnectionType::try_from_ip` (and `FromStr`) accepts an IP without a port (5555).
  `Display` prints `192.168.1.10:5555` instead of `ip:192.168.1.10:5555`, like adb.
- `Adb::list_devices()` returns the devices in any state; `AdbDevice` has `serial` (was
  `name`), `state: DeviceState` (was `connected: bool`), optional `product`/`model`/`device`,
  and new `usb` and `transport_id` fields; devices are addressed by serial.
  *Why:* offline and unauthorized devices were skipped, and a transport id greater than 255
  made the whole listing fail.
- `Adb` methods no longer take a `debug` argument: use `Adb::with_debug`. `Adb` is a struct.
  `Adb::exec(&addr, ...)` takes a reference and checks the exit status.
- `Adb::disconnect_all`, `kill_server`, `start_server` return `Result<()>`.
- `Client::disconnect()` returns `Result<()>`, and fails for non-tcp/ip connections.
  *Why:* in 1.x it ran `adb disconnect` without arguments for USB devices, which disconnected
  every tcp/ip device.
- `Client::root()`/`unroot()` return `Result<()>`, wait for adbd to restart (instead of
  sleeping one second), and fail on production builds.
- `Client::get_seriano` is renamed `get_serialno`; `Client::try_disconnect` is removed.

**Package manager**

- `SimplePackageReader::new(data)` no longer needs the SDK version; its methods return `Vec`s
  instead of `Option<Vec>`; `get_user_id` is renamed `get_app_id`; versions are `i64`
  (`get_version_code`, `Package::version_code`, `UninstallOptions::version_code`).
  *Why:* the section header is found in any Android version; long version codes don't fit in
  an `i32`; `userId` is called `appId` since Android 10.
- `ListPackageFilter` fields renamed: `show_only_enabled` (was `show_only_enabed`) and
  `show_only_3rd_party` (was `show_only3rd_party`).
- `PackageManager::grant`/`revoke` fail with `PackageNotFoundError` for an unknown package.
  *Why:* Android 14 exits with 0, without a message.
- `PackageManager::uninstall` of a system app only removes its updates, and fails
  (`DELETE_FAILED_INTERNAL_ERROR`) if it has none: that is what Android does. Use
  `Client::remove_system_app` to remove a system app.

**Crate**

- MSRV 1.81 (required by `cidr-utils`, feature `scanner`). The CI checks it.
- New features: `clipboard` (`Client::copy_screencap`, with `arboard` and `image`) and
  `emulator-tests`; `scanner` now also covers `cidr-utils` and `itertools`.
  *Why:* a clipboard and an image decoder are heavy dependencies for one method.
- Removed dependencies: `anyhow`, `cmd_lib`, `crossbeam`, `dirs`, `lazy_static`, `lru`,
  `num_cpus`, `once_cell`, `rustix`, `signal-hook`, `strum_macros`, `time` (unused, test-only,
  or replaced by `std`). New: `miniz_oxide` (to read APKs).
- `simple-cmd` 0.0.23. *Why:* it fixes a busy-wait loop that used about 60% of a CPU core while
  waiting for any command, and a panic when a program could not be started.
- `prelude` and `traits` are private (they only had crate-internal items). The main types are
  re-exported at the crate root (`radb_client::Client`); `radb_client::types::*` still works.
- Every public item is documented (`#![warn(missing_docs)]`).

### Added

- **System apps.** `Client::install_system_app` installs or updates a system app whatever its
  state, `Client::remove_system_app` removes it completely, `PackageManager::system_app_info`
  says where its copies are, `PackageManager::uninstall_system_updates` removes its updates.
  *Why:* installing another version of a system app (higher or lower, debug or release) failed
  in many different ways; see the README and the list of Android behaviors below. The installer
  checks the privileged permissions before changing the device, rolls back if the device does
  not boot, and removes duplicate system copies.
- `examples/system_app.rs`: a command line tool for `info`, `install` and `remove`.
- `ApkManifest::read`: the package name, version, permissions and native ABIs of an APK, read
  on the host without the Android SDK.
- `Client::reboot_and_wait`: reboot and wait for the *new* boot (a new boot id).
  *Why:* right after `adb reboot` the old boot can still answer, and `wait_for_device` returned
  too early.
- `Shell::exec_with`, `exec_script`, `exec_raw`, `spawn`; `utils::shell_quote`;
  `ConnectionType::serial`, `ConnectionType::emulator`; `DeviceState`; `DumpsysOptions`;
  `Client::am()`, `Client::pm()`; `RebootType::Userspace`; `Wakefulness::Dozing`;
  `PackageFlags::Debuggable`; `Extra::put_float_extra`; `PropType::{Uint, Double, Size}`.

### Fixed

- String array extras (`Extra::esa`) were sent as `--efa` (float arrays); commas in the values
  are now escaped.
- `Intent::flags` was ignored; `am crash` ignored the user id; `UserOption::All`/`Current` were
  missing `--user`.
- `Client::connect` dropped the port and always connected to 5555.
- `Shell::file_mode` read the octal mode (`755`) as a decimal number.
- `ChmodOptions::verbose` was ignored.
- `InstallOptions` and `ListPackageFilter` sent an option and its value as one argument
  (`"--user 10"`); `send_swipe` sent the four coordinates as one argument. They only worked
  because the device shell split them again.
- `utils::android_home` returned `platform-tools` instead of the SDK directory when
  `ANDROID_HOME` was not set; `ANDROID_SDK_ROOT` is now supported.
- `ApkAnalyzer::summary` panicked on unexpected output; the `ApkAnalyzer` methods ignored the
  exit status.
- `wait_for_device` sent `KEYCODE_NUM_LOCK` (keyevent 143) to the device.
- `get_wakefulness`, `is_screen_on` and `ApkAnalyzer::classes` piped through `sed`, `egrep` and
  `grep` on the host (missing on Windows, different on macOS); they now parse in Rust.
- `enable_verity` printed to stdout; `screen_mirror` forced debug logging.
- The `Scanner` rooted every device it found (restarting their adbd), and disconnected devices
  that were connected before the scan.

### Android behaviors handled

Observed on Android 14 and 15 emulators and on an Android 14 device, and now handled by the
library. They explain several of the changes above.

- `adb shell` joins its arguments with spaces and the device shell parses them again;
  `adb logcat`, `adb install` and `adb uninstall` escape their arguments themselves (they are
  not quoted twice).
- Exit codes are reported since Android 7 (shell protocol v2); before, `am` errors are found in
  the output (`Error:` lines).
- A transport id changes every time the connection is re-established (`adb root`, reboot).
- `adb root` can exit with 1 (`timeout expired while waiting for device`) while adbd is still
  restarting.
- `pm uninstall-system-updates` prints `Success` but exits with 1 (Android 14, 15), and resets
  the app data when the system copy is older than the update.
- `pm grant`/`revoke` of an unknown package exit with 0 (Android 14).
- `pm list permissions -f` can crash halfway and print a partial list (Android 14):
  `dumpsys package permissions` is used instead.
- `adb install -d` accepts an update older than the system copy; the package manager deletes it
  at the next boot.
- `pm uninstall --user 0` of a system app only hides it for user 0, and deletes its data.
- Replacing the system copy keeps the app data, also when the signature changes or the version
  is lower (`signature changed; retaining data`). Without the update files, the package manager
  reverts to the system copy and keeps the data (`Expected better ... reverting to system`).
- The package manager caches the parsed APKs (`/data/system/package_cache`) and uses an entry
  while it is newer than the APK directory: a copied or restored APK with an old modification
  time is ignored in favor of the cached one.
- With `ro.control_privapp_permissions=enforce`, a privileged permission missing from the
  allowlist makes `system_server` crash at every boot. adbd starts before it, so the device can
  be repaired from adb. The allowlist is only enforced for the permissions defined by the
  platform (`sourcePackage=android`): a privileged permission defined by an app (e.g. Netflix's
  `com.netflix.ninja.permission.DET`) never stops the boot, so it is not checked.
- After a reboot a network (`adb connect`) device is not always reconnected by the adb server:
  `Client::reboot_and_wait` runs `adb connect` again while it waits.
- The package manager does not extract the native libraries of system apps: they must be in
  `<app dir>/lib/<isa>/` (or stored uncompressed).
- With two system copies of a package, the package manager silently runs the higher version.
- On devices with dynamic partitions `adb remount` uses overlayfs (`/mnt/scratch`); the first
  remount may ask for a reboot.

### Tests and CI

- Unit tests need no device: the parsers are tested on outputs captured from Android 14 and 15
  emulators (`test_files/fixtures`) and on generated APKs (`test_files/system_app/build.sh`).
- The device tests are integration tests (`tests/emulator`, feature `emulator-tests`). They run
  on an emulator, one at a time; a physical device needs `RADB_TEST_SERIAL` and
  `RADB_TEST_ALLOW_PHYSICAL=1`. The system app tests (`--include-ignored`) cover new system
  copies, updates, key changes, downgrades, hidden apps, privileged permissions, a real boot
  loop with rollback, duplicate copies and native libraries.
- The doctests no longer connect to devices (`no_run`).
- The system app installer was also run on an Android 14 TV device (A/B, dynamic partitions,
  overlayfs, `enforce` allowlist): new system copy, update, key change, removal, and the
  replacement of a platform-signed `android.uid.system` app placed directly in `priv-app/`.
- GitHub Actions: fmt, clippy and unit tests on Linux and macOS, MSRV check, and the emulator
  tests on an API 35 emulator.
