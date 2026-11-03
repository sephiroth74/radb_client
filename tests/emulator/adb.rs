use radb_client::{ConnectionType, DeviceState, Error};

use crate::common::{adb, device, serial};

#[test]
fn version() {
    let version = adb().version().unwrap();
    assert!(
        version.chars().next().unwrap().is_ascii_digit(),
        "{version}"
    );
}

#[test]
fn list_devices() {
    let _device = device();
    let devices = adb().list_devices().unwrap();
    let emulator = devices
        .iter()
        .find(|d| d.serial == serial())
        .expect("the device under test is not listed");
    assert_eq!(emulator.state, DeviceState::Device);
    assert_eq!(emulator.addr, ConnectionType::serial(serial()));
    assert!(emulator.model.is_some());
    assert!(emulator.transport_id.is_some());
}

#[test]
fn exec() {
    let device = device();
    let output = adb().exec(&device.addr, ["get-state"], None, None).unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "device");

    let err = adb()
        .exec(&device.addr, ["shell", "exit 3"], None, None)
        .unwrap_err();
    assert!(matches!(err, Error::CommandFailed(_)), "{err:?}");
    assert_eq!(err.exit_code(), Some(3));
}
