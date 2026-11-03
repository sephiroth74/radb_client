use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};

use radb_client::{
    Client, ConnectionType, KeyCode, LogcatLevel, LogcatOptions, LogcatTag, Wakefulness,
};

use crate::common::{adb, device, serial, tmp_path, TEST_APK, TEST_PACKAGE};

#[test]
fn is_connected() {
    let device = device();
    assert!(device.is_connected());

    let missing = Client::new(adb(), ConnectionType::emulator(5998), false);
    let start = Instant::now();
    assert!(!missing.is_connected());
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn serialno() {
    let device = device();
    assert_eq!(device.get_serialno().unwrap(), serial());
}

#[test]
fn wait_for_device() {
    let device = device();
    device
        .wait_for_device(Some(Duration::from_secs(10)))
        .unwrap();
}

#[test]
fn wakefulness() {
    let device = device();
    device
        .shell()
        .send_keyevent(KeyCode::KEYCODE_WAKEUP, None, None)
        .unwrap();
    assert_eq!(device.get_wakefulness().unwrap(), Wakefulness::Awake);
    assert!(device.is_awake().unwrap());
}

/// The serial survives the adbd restarts (a transport id would not).
#[test]
fn root_and_unroot() {
    let device = device();
    device.unroot().unwrap();
    assert!(!device.is_root().unwrap());

    device.root().unwrap();
    assert!(device.is_root().unwrap());
    assert_eq!(device.shell().whoami().unwrap(), "root");
    // root() is a no-op when adbd already runs as root
    device.root().unwrap();

    // files only root can read
    assert!(device.get_mac_address().is_ok());
    assert!(device.get_wlan_address().is_ok());

    device.unroot().unwrap();
    assert_eq!(device.shell().whoami().unwrap(), "shell");
}

#[test]
fn boot_id() {
    let device = device();
    let boot_id = device.get_boot_id().unwrap();
    assert!(!boot_id.is_nil());
    assert_eq!(device.get_boot_id().unwrap(), boot_id);
}

#[test]
fn push_and_pull() {
    let device = device();
    let local_dir = std::env::temp_dir().join(format!("radb-{}", std::process::id()));
    std::fs::create_dir_all(&local_dir).unwrap();
    let local = local_dir.join("source file.txt");
    std::fs::write(&local, "hello radb\n").unwrap();

    let remote = tmp_path("push.txt");
    device.push(&local, &remote).unwrap();
    assert_eq!(device.shell().cat(&remote).unwrap(), b"hello radb\n");

    let pulled = local_dir.join("pulled.txt");
    device.pull(&remote, &pulled).unwrap();
    assert_eq!(std::fs::read_to_string(&pulled).unwrap(), "hello radb\n");

    device.shell().rm(&remote, &[]).unwrap();
    assert!(
        device.pull(&remote, &pulled).is_err(),
        "pulling a missing file must fail"
    );
    std::fs::remove_dir_all(&local_dir).unwrap();
}

#[test]
fn install_and_uninstall() {
    let device = device();
    if device.pm().is_installed(TEST_PACKAGE, None).unwrap() {
        device.uninstall(TEST_PACKAGE, None).unwrap();
    }
    assert!(!device.pm().is_installed(TEST_PACKAGE, None).unwrap());
    assert!(
        device.uninstall(TEST_PACKAGE, None).is_err(),
        "uninstalling a missing package must fail"
    );

    device.install(TEST_APK, None).unwrap();
    assert!(device.pm().is_installed(TEST_PACKAGE, None).unwrap());
    assert!(device.install("/nonexistent.apk", None).is_err());
}

#[test]
fn logcat() {
    let device = device();
    device.clear_logcat().unwrap();
    device
        .shell()
        .exec(["log", "-t", "RadbTest", "hello from radb"])
        .unwrap();

    let options = LogcatOptions {
        dump: true,
        tags: Some(vec![LogcatTag {
            name: "RadbTest".to_string(),
            level: LogcatLevel::Info,
        }]),
        ..Default::default()
    };
    let logs = device.logcat(options, None).unwrap();
    assert!(logs.contains("hello from radb"), "{logs}");

    // without dump, logcat runs until the timeout
    let start = Instant::now();
    let options = LogcatOptions {
        timeout: Some(Duration::from_secs(1)),
        ..Default::default()
    };
    device.logcat(options, None).unwrap();
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn save_screencap() {
    let device = device();
    let path = std::env::temp_dir().join(format!("radb-screencap-{}.png", std::process::id()));
    device.save_screencap(File::create(&path).unwrap()).unwrap();

    let mut header = [0u8; 8];
    File::open(&path).unwrap().read_exact(&mut header).unwrap();
    assert_eq!(&header, b"\x89PNG\r\n\x1a\n");
    std::fs::remove_file(&path).unwrap();
}

#[test]
#[ignore = "reboots the emulator"]
fn reboot() {
    let device = device();
    let boot_id = device.get_boot_id().unwrap();
    device.reboot(None).unwrap();
    device
        .wait_for_device(Some(Duration::from_secs(300)))
        .unwrap();
    assert_ne!(device.get_boot_id().unwrap(), boot_id);
}
