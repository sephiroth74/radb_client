//! System apps: these tests reboot the emulator many times.

use std::time::Duration;

use radb_client::{
    Client, Error, PrivappPermissionsPolicy, SystemAppInstallMethod, SystemAppInstallOptions,
    SystemAppRemoveOptions, SystemAppStrategy,
};

use crate::common::device;

const PACKAGE: &str = "it.sephiroth.radb.sysapp";
const DIR: &str = "/system/priv-app/RadbSysapp";

fn apk(name: &str) -> String {
    format!(
        "{}/test_files/system_app/{name}.apk",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn options(strategy: SystemAppStrategy) -> SystemAppInstallOptions {
    SystemAppInstallOptions {
        strategy,
        target_dir: Some(DIR.to_string()),
        ..Default::default()
    }
}

/// Remove the test app, whatever its state.
fn clean(client: &Client) {
    match client.pm().system_app_info(PACKAGE) {
        Err(Error::PackageNotFoundError(_)) => {}
        Ok(_) => client
            .remove_system_app(PACKAGE, SystemAppRemoveOptions::default())
            .unwrap(),
        Err(err) => panic!("{err}"),
    }
    client.root().unwrap();
}

fn data_marker() -> String {
    format!("/data/data/{PACKAGE}/radb-marker")
}

fn write_data_marker(client: &Client) {
    let marker = data_marker();
    client
        .shell()
        .exec_script(&format!(
            "echo radb > {marker} && chown $(stat -c %u /data/data/{PACKAGE}):$(stat -c %g /data/data/{PACKAGE}) {marker}"
        ))
        .unwrap();
}

#[test]
#[ignore = "reboots the emulator"]
fn lifecycle() {
    let device = device();
    clean(&device);

    // not installed: a new system copy
    let report = device
        .install_system_app(apk("sysapp-v2-debug"), options(SystemAppStrategy::Auto))
        .unwrap();
    assert_eq!(report.method, SystemAppInstallMethod::SystemCopy);
    assert_eq!(report.before, None);
    let system_copy = report.after.system_copy.clone().unwrap();
    assert_eq!(
        (system_copy.code_path.as_str(), system_copy.version_code),
        (DIR, Some(2))
    );
    assert!(report.after.privileged);
    write_data_marker(&device);

    // same key, higher version: an update, without reboot
    let boot_id = device.get_boot_id().unwrap();
    let report = device
        .install_system_app(apk("sysapp-v3-debug"), options(SystemAppStrategy::Auto))
        .unwrap();
    assert_eq!(report.method, SystemAppInstallMethod::Update);
    assert_eq!(device.get_boot_id().unwrap(), boot_id);
    assert_eq!(report.after.update.as_ref().unwrap().version_code, Some(3));
    assert_eq!(
        report.after.system_copy.as_ref().unwrap().version_code,
        Some(2)
    );

    // another key: the update is removed, the system copy replaced
    let report = device
        .install_system_app(apk("sysapp-v3-release"), options(SystemAppStrategy::Auto))
        .unwrap();
    assert_eq!(report.method, SystemAppInstallMethod::SystemCopy);
    assert_eq!(report.after.update, None);
    assert_eq!(report.after.active.version_code, Some(3));
    assert_ne!(report.after.active.signatures, system_copy.signatures);
    // the package manager keeps the data when the signature changes
    assert!(device.shell().exists(data_marker()).unwrap());

    // lower than the system copy, and hidden for user 0 (`pm uninstall --user 0` also
    // deletes the user data): replaced, and installed again for user 0
    device
        .shell()
        .exec(["pm", "uninstall", "--user", "0", PACKAGE])
        .unwrap();
    let report = device
        .install_system_app(apk("sysapp-v1-debug"), options(SystemAppStrategy::Auto))
        .unwrap();
    // depending on the case the package manager resets the hidden state itself, or the
    // installer runs `pm install-existing`: in the end it is installed for user 0
    assert!(
        report.unhidden_users.is_empty() || report.unhidden_users == [0],
        "{report:?}"
    );
    assert!(report.after.hidden_for_users().is_empty());
    assert!(device.pm().is_installed(PACKAGE, None).unwrap());
    assert_eq!(report.method, SystemAppInstallMethod::SystemCopy);
    assert_eq!(report.after.active.version_code, Some(1));

    // forced replacement of an updated app, wiping the data
    device.install(apk("sysapp-v2-debug"), None).unwrap();
    assert!(device
        .pm()
        .system_app_info(PACKAGE)
        .unwrap()
        .update
        .is_some());
    let report = device
        .install_system_app(
            apk("sysapp-v2-debug"),
            SystemAppInstallOptions {
                wipe_data: true,
                ..options(SystemAppStrategy::ReplaceSystemCopy)
            },
        )
        .unwrap();
    assert_eq!(report.method, SystemAppInstallMethod::SystemCopy);
    assert_eq!(report.after.update, None);
    assert_eq!(report.after.active.code_path, DIR);
    assert!(!device.shell().exists(data_marker()).unwrap());

    // complete removal: the package and its data
    device
        .remove_system_app(PACKAGE, SystemAppRemoveOptions::default())
        .unwrap();
    assert!(matches!(
        device.pm().system_app_info(PACKAGE),
        Err(Error::PackageNotFoundError(_))
    ));
    assert!(!device.shell().exists(DIR).unwrap());
    assert!(!device
        .shell()
        .exists(format!("/data/data/{PACKAGE}"))
        .unwrap());
}

#[test]
#[ignore = "reboots the emulator"]
fn privileged_permissions() {
    let device = device();
    clean(&device);

    // refused before changing the device
    let boot_id = device.get_boot_id().unwrap();
    match device.install_system_app(
        apk("sysapp-v4-privileged"),
        options(SystemAppStrategy::Auto),
    ) {
        Err(Error::PrivappPermissionsError {
            package,
            permissions,
        }) => {
            assert_eq!(package, PACKAGE);
            assert_eq!(permissions, ["android.permission.INSTALL_PACKAGES"]);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(device.get_boot_id().unwrap(), boot_id);
    assert!(!device.shell().exists(DIR).unwrap());

    // allowlisted
    let report = device
        .install_system_app(
            apk("sysapp-v4-privileged"),
            SystemAppInstallOptions {
                privapp_permissions: PrivappPermissionsPolicy::Allowlist,
                ..options(SystemAppStrategy::Auto)
            },
        )
        .unwrap();
    assert_eq!(
        report.allowlisted_permissions,
        ["android.permission.INSTALL_PACKAGES"]
    );
    let granted = device
        .pm()
        .install_permissions(PACKAGE)
        .unwrap()
        .into_iter()
        .any(|p| p.name == "android.permission.INSTALL_PACKAGES" && p.granted);
    assert!(granted);

    let allowlist = format!("/system/etc/permissions/privapp-permissions-radb-{PACKAGE}.xml");
    assert!(device.shell().exists(&allowlist).unwrap());
    device
        .remove_system_app(PACKAGE, SystemAppRemoveOptions::default())
        .unwrap();
    assert!(!device.shell().exists(&allowlist).unwrap());
}

/// A privileged permission outside the allowlist makes system_server crash at every
/// boot: the previous system copy is restored.
#[test]
#[ignore = "reboots the emulator, and waits for a boot loop"]
fn rollback_when_the_device_does_not_boot() {
    let device = device();
    clean(&device);
    device
        .install_system_app(apk("sysapp-v2-debug"), options(SystemAppStrategy::Auto))
        .unwrap();

    let err = device
        .install_system_app(
            apk("sysapp-v4-privileged"),
            SystemAppInstallOptions {
                privapp_permissions: PrivappPermissionsPolicy::Ignore,
                boot_timeout: Duration::from_secs(90),
                ..options(SystemAppStrategy::ReplaceSystemCopy)
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("was restored"), "{err}");

    device.root().unwrap();
    let info = device.pm().system_app_info(PACKAGE).unwrap();
    assert_eq!(info.active.code_path, DIR);
    assert_eq!(info.active.version_code, Some(2));

    device
        .remove_system_app(PACKAGE, SystemAppRemoveOptions::default())
        .unwrap();
}

/// Two system copies of the package (an APK placed directly in priv-app/ and a
/// directory), and native libraries.
#[test]
#[ignore = "reboots the emulator"]
fn duplicates_and_native_libraries() {
    let device = device();
    clean(&device);

    // two copies with the same version: the package manager runs one of them
    let shell = device.shell();
    device.root().unwrap();
    device.remount(false).unwrap();
    shell.exec(["mkdir", "-p", DIR]).unwrap();
    device
        .push(apk("sysapp-v3-debug"), format!("{DIR}/RadbSysapp.apk"))
        .unwrap();
    device
        .push(apk("sysapp-v3-debug"), "/system/priv-app/RadbFlat.apk")
        .unwrap();
    shell
        .exec_script(&format!(
            "chmod 755 {DIR} && chmod 644 {DIR}/RadbSysapp.apk /system/priv-app/RadbFlat.apk"
        ))
        .unwrap();
    device.reboot_and_wait(Duration::from_secs(300)).unwrap();
    device.root().unwrap();
    let active = device
        .pm()
        .system_app_info(PACKAGE)
        .unwrap()
        .active
        .code_path;
    assert!(
        active == DIR || active == "/system/priv-app/RadbFlat.apk",
        "{active}"
    );

    // a lower version: after the reboot the package manager runs the other copy, which is removed
    let report = device
        .install_system_app(
            apk("sysapp-v2-debug"),
            SystemAppInstallOptions {
                strategy: SystemAppStrategy::ReplaceSystemCopy,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.removed_duplicates.len(), 1, "{report:?}");
    assert_eq!(report.after.active.version_code, Some(2));
    assert!(
        !device
            .shell()
            .exists("/system/priv-app/RadbFlat.apk")
            .unwrap()
            || !device.shell().exists(DIR).unwrap()
    );
    let dir = report.after.active.code_path.clone();

    // native libraries are extracted for the device ABIs
    let report = device
        .install_system_app(
            apk("sysapp-v5-native"),
            SystemAppInstallOptions {
                strategy: SystemAppStrategy::ReplaceSystemCopy,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(report.after.active.code_path, dir);
    let abi = device.shell().getprop("ro.product.cpu.abi").unwrap();
    let isa = match abi.as_str() {
        "arm64-v8a" => "arm64",
        "x86_64" => "x86_64",
        other => panic!("unexpected abi {other}"),
    };
    assert!(device
        .shell()
        .is_file(format!("{dir}/lib/{isa}/libradb.so"))
        .unwrap());

    device
        .remove_system_app(PACKAGE, SystemAppRemoveOptions::default())
        .unwrap();
    assert!(!device.shell().exists(&dir).unwrap());
}
