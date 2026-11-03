use std::ops::Deref;
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use std::time::Duration;

use radb_client::{Adb, AdbInstallOptions, Client, ConnectionType};

pub const TEST_APK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test_files/app-debug.apk");
pub const TEST_PACKAGE: &str = "it.sephiroth.android.app.app";
pub const TEST_ACTIVITY: &str = "it.sephiroth.android.app.app/.MainActivity";
pub const TMP_DIR: &str = "/data/local/tmp";

/// The tests change the device state (root, settings, packages...): they run
/// one at a time, whatever `--test-threads` says.
static LOCK: Mutex<()> = Mutex::new(());

/// The device under test, locked for the duration of a test.
pub struct TestDevice {
    pub client: Client,
    _guard: MutexGuard<'static, ()>,
}

impl Deref for TestDevice {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

/// Lock and return the device under test.
///
/// It is `RADB_TEST_SERIAL`, or the first running emulator. A serial that is not
/// an emulator is refused unless `RADB_TEST_ALLOW_PHYSICAL=1`: the tests change
/// the device state.
pub fn device() -> TestDevice {
    init_tracing();
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let client = Client::new(adb(), ConnectionType::serial(serial()), true);
    client
        .wait_for_device(Some(Duration::from_secs(120)))
        .expect("the device did not complete the boot");
    TestDevice {
        client,
        _guard: guard,
    }
}

pub fn adb() -> Adb {
    Adb::new().expect("adb not found in PATH").with_debug(true)
}

pub fn serial() -> String {
    static SERIAL: OnceLock<String> = OnceLock::new();
    SERIAL
        .get_or_init(|| {
            if let Ok(serial) = std::env::var("RADB_TEST_SERIAL") {
                let allow_physical = std::env::var("RADB_TEST_ALLOW_PHYSICAL").as_deref() == Ok("1");
                assert!(
                    serial.starts_with("emulator-") || allow_physical,
                    "RADB_TEST_SERIAL={serial} is not an emulator, and these tests change the device \
                     state. Set RADB_TEST_ALLOW_PHYSICAL=1 to run them anyway."
                );
                return serial;
            }
            adb()
                .list_devices()
                .expect("adb devices failed")
                .into_iter()
                .find(|d| d.serial.starts_with("emulator-") && d.is_online())
                .map(|d| d.serial)
                .expect("no emulator running: start one (see README) or set RADB_TEST_SERIAL")
        })
        .clone()
}

fn init_tracing() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("simple_cmd=trace"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .try_init();
    });
}

/// Install the test app, if it is not installed.
pub fn install_test_app(client: &Client) {
    if !client.pm().is_installed(TEST_PACKAGE, None).unwrap() {
        let options = AdbInstallOptions {
            replace: true,
            ..Default::default()
        };
        client
            .install(TEST_APK, Some(options))
            .expect("failed to install the test app");
    }
}

/// A unique path in the device temp directory, with a space in the name.
pub fn tmp_path(name: &str) -> String {
    format!("{TMP_DIR}/radb test {}-{name}", std::process::id())
}
