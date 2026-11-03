# radb_client

Android **adb** client for Rust. It runs the `adb` executable found in your `PATH`
and parses its output.

[![crates.io](https://img.shields.io/crates/v/radb_client.svg)](https://crates.io/crates/radb_client/)

Upgrading from 1.x? See the [CHANGELOG](CHANGELOG.md).

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

Every command that exits with a non-zero status returns `Error::CommandFailed`, with the
command line, the exit code and the output.

### Shell commands

`adb shell` joins its arguments with spaces and the device shell parses them again.
`Shell::exec` quotes each argument, so it reaches the device program unchanged:

```rust,no_run
# use radb_client::{Client, ConnectionType};
# let client = Client::try_from(ConnectionType::emulator(5554)).unwrap();
let shell = client.shell();
shell.exec(["touch", "/sdcard/Download/my file.txt"])?;  // one file, not two
shell.exec_script("ls /system/app | wc -l")?;            // a script: pipes, variables...
# Ok::<(), radb_client::Error>(())
```

`Shell::exec_raw` keeps the plain `adb shell` behavior (no quoting, no exit status check).

## Features

- `scanner`: scan a network range for adb devices (`radb_client::scanner::Scanner`).
- `clipboard`: `Client::copy_screencap`, copy a screenshot to the host clipboard.

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

The unit tests parse outputs captured from a device (`test_files/fixtures`).

The integration tests in `tests/emulator` need a running emulator. They change its state
(root, settings, packages...), so they run one at a time and never pick a physical
device unless you ask for it.

```sh
# once: an API 35 image (google_apis, so that `adb root` works)
android sdk install system-images/android-35/google_apis/arm64-v8a   # x86_64 on Intel
avdmanager create avd -n radb-test -k "system-images;android-35;google_apis;arm64-v8a" -d pixel_6

emulator -avd radb-test -no-window -no-audio -no-snapshot &
adb wait-for-device

cargo test --features emulator-tests,scanner --test emulator
cargo test --features emulator-tests --test emulator -- --include-ignored   # also reboots the emulator
```

The tests use the first running emulator, or the device in `RADB_TEST_SERIAL`
(a physical device also needs `RADB_TEST_ALLOW_PHYSICAL=1`).

The CI (`.github/workflows/ci.yml`) runs them on an API 35 emulator, reboot included.
