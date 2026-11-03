# Changelog

## 2.0.0

### Breaking changes

**Errors and exit codes**
- Commands that exit with a non-zero status now fail with `Error::CommandFailed(CommandFailure)`:
  the command line, exit code, stdout and stderr. In 1.x most methods returned `Ok` with an
  empty or partial output.
- `Error` is `#[non_exhaustive]`. `ParseInputError` is replaced by `ParseError(String)`, which
  says what could not be parsed. New variants: `Timeout`, `ToolNotFound`, `Utf8Error`.
- `utils` and `cmdline_tools::ApkAnalyzer` return `radb_client::Result` instead of `anyhow::Result`.
- `radb_client::result::Result` is now `radb_client::Result` (also `radb_client::error::Result`).

**Shell**
- `Shell::exec(args)` quotes each argument and checks the exit status. The 1.x
  `exec(args, cancel, timeout)` maps to:
  - `exec_with(args, cancel, timeout)`: quoted and checked;
  - `exec_raw(args, cancel, timeout)`: the 1.x behavior (no quoting, no check);
  - `exec_script(script)`: a shell script (pipes, redirections...), checked.
- The `try_*` methods (`try_exec`, `try_send_keyevent`...) are removed. They never waited for the
  command, so they returned `Ok(None)`, and they panicked if the spawn failed. Use
  `Shell::spawn(args)`, which returns the `std::process::Child`.
- `Shell` is `Copy`. `Shell::am()`/`pm()` return handles that borrow the client, not the
  shell, so `let am = client.shell().am();` now compiles. There are also the shortcuts
  `Client::am()` and `Client::pm()`.
- Device paths are `impl AsRef<str>` instead of `rustix::path::Arg`. Local paths are
  `impl AsRef<Path>`.
- `which()` returns `Result<Option<String>>`. `get_command_path()` and its cache are removed.
- `ls(path, &[args])` and `rm(path, &[args])` take slices.
- `dumpsys(service, args, DumpsysOptions)` replaces the 7 positional arguments.
- These methods now return `Result<()>`: `chmod`, `save_screencap`, `screen_record`,
  `screen_mirror`, `Client::pull`, `Client::push`, `Client::bug_report`.
- `Client::logcat` returns the log text, `Result<String>`.
- `build_version_release()` returns a `String`, because releases like `8.1.0` are not integers.

**Devices and connections**
- `ConnectionType` has new variants, `Serial(String)` (`-s serial`) and `Emulator` (`-e`), so it
  is `Clone` and no longer `Copy`. `Transport` takes a `u64`. The enum is `#[non_exhaustive]`.
  Prefer `Serial`, or `ConnectionType::emulator(5554)`: a transport id changes every time adbd
  restarts (`adb root`, reboot).
- `ConnectionType::try_from_ip` accepts an IP without a port (it defaults to 5555).
  `Display` prints `192.168.1.10:5555` instead of `ip:192.168.1.10:5555`.
- `Adb::list_devices()` lists the devices in any state (offline, unauthorized, recovery...).
  `AdbDevice` has `serial` (was `name`) and `state: DeviceState` (was `connected: bool`).
  `product`, `model` and `device` are `Option`s, and there are new `usb` and `transport_id`
  fields. Devices are addressed by serial.
- `Adb` methods no longer take a `debug` argument: use `Adb::with_debug`. `Adb` is a struct and
  no longer a tuple struct. `Adb::exec(&addr, ...)` takes a reference and checks the exit status.
- `Adb::disconnect_all`, `kill_server` and `start_server` return `Result<()>`.
- `Client::disconnect()` returns `Result<()>`, and fails for non-tcp connections. In 1.x it
  ran `adb disconnect` without arguments, which disconnected every device.
- `Client::root()`/`unroot()` return `Result<()>`. They wait for adbd to restart instead of
  sleeping for one second, and fail on production builds.
- `Client::get_seriano` is renamed `get_serialno`.
- `Client::try_disconnect` is removed.

**Package manager**
- `SimplePackageReader::new(data)` no longer needs the SDK version. Its methods return `Vec`s
  instead of `Option<Vec>`. `get_user_id` is renamed `get_app_id`. `get_version_code` returns
  `i64`, as does `Package::version_code`.
- `ListPackageFilter` fields renamed: `show_only_enabled` (was `show_only_enabed`) and
  `show_only_3rd_party` (was `show_only3rd_party`).
- `UninstallOptions::version_code` is an `i64`.

**Crate**
- MSRV 1.81 (required by `cidr-utils`, feature `scanner`).
- New features: `clipboard` (`Client::copy_screencap`, pulls in `arboard` and `image`) and
  `emulator-tests`. The `scanner` feature now also covers `cidr-utils` and `itertools`.
- Removed dependencies: `anyhow`, `cmd_lib`, `crossbeam`, `dirs`, `lazy_static`, `lru`,
  `num_cpus`, `once_cell`, `rustix`, `signal-hook`, `strum_macros`, `time`.
- `prelude` and `traits` are private (they only had crate-internal items).
- The main types are re-exported at the crate root (`radb_client::Client`...).
  `radb_client::types::*` still works.
- The `Scanner` no longer roots the devices it finds, and only disconnects the devices it
  connected itself.

### Fixes
- String array extras (`Extra::esa`) were sent as `--efa` (float arrays).
- `Intent::flags` was ignored. `am crash` ignored the user id, and `UserOption::All`/`Current`
  were missing `--user`.
- `Client::connect` dropped the port and always connected to 5555.
- `list_devices` failed on transport ids greater than 255, and skipped offline/unauthorized devices.
- `Shell::file_mode` read the octal mode as a decimal number.
- `ChmodOptions::verbose` was ignored.
- `InstallOptions` and `ListPackageFilter` sent `--user`, `--pkg`... and their value as a single argument.
- `send_swipe` sent the four coordinates as a single argument.
- `utils::android_home` returned `platform-tools` instead of the SDK directory when
  `ANDROID_HOME` was not set.
- `ApkAnalyzer::summary` panicked on unexpected output.
- `wait_for_device` sent `KEYCODE_NUM_LOCK` (keyevent 143) to the device.
- `get_wakefulness`, `is_screen_on` and `ApkAnalyzer::classes` piped through `sed`/`egrep`/`grep`
  on the host. They now parse in Rust.
- `enable_verity` printed to stdout. `screen_mirror` forced debug logging.

### Tests
- Unit tests no longer need a device: the parsers are tested on outputs captured from an API 35
  emulator (`test_files/fixtures`).
- The device tests are now integration tests (`tests/emulator`, feature `emulator-tests`) that
  run on an emulator, one at a time. See the README.
- The doctests no longer connect to devices (`no_run`).
