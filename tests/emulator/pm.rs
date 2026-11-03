use radb_client::{
    Error, InstallOptions, ListPackageDisplayOptions, ListPackageFilter, PackageFlags,
    UninstallOptions,
};

use crate::common::{device, install_test_app, tmp_path, TEST_APK, TEST_PACKAGE};

const NOTIFICATIONS: &str = "android.permission.POST_NOTIFICATIONS";

#[test]
fn path_and_is_installed() {
    let device = device();
    install_test_app(&device);
    let pm = device.pm();

    let path = pm.path(TEST_PACKAGE, None).unwrap();
    assert!(
        path.starts_with("/data/app/") && path.ends_with("/base.apk"),
        "{path}"
    );
    assert!(pm.is_installed(TEST_PACKAGE, None).unwrap());

    assert!(matches!(
        pm.path("com.nonexistent", None),
        Err(Error::PackageNotFoundError(_))
    ));
    assert!(!pm.is_installed("com.nonexistent", None).unwrap());
}

#[test]
fn list_packages() {
    let device = device();
    install_test_app(&device);
    let filter = ListPackageFilter {
        show_only_3rd_party: true,
        ..Default::default()
    };
    let packages = device
        .pm()
        .list_packages(filter, ListPackageDisplayOptions::default(), None)
        .unwrap();
    let app = packages
        .iter()
        .find(|p| p.package_name == TEST_PACKAGE)
        .expect("test app not listed");
    assert_eq!(app.version_code, Some(1));
    assert!(app.uid.is_some());
    assert!(app.file_name.is_some());
    assert!(!packages
        .iter()
        .any(|p| p.package_name == "com.android.settings"));

    let filtered = device
        .pm()
        .list_packages(
            ListPackageFilter::default(),
            ListPackageDisplayOptions::default(),
            Some("sephiroth"),
        )
        .unwrap();
    assert_eq!(filtered.len(), 1);
}

#[test]
fn permissions() {
    let device = device();
    install_test_app(&device);
    let pm = device.pm();

    assert!(pm
        .requested_permissions(TEST_PACKAGE)
        .unwrap()
        .contains(&NOTIFICATIONS.to_string()));
    assert!(!pm.install_permissions(TEST_PACKAGE).unwrap().is_empty());

    let granted = |pm: &radb_client::PackageManager| {
        pm.runtime_permissions(TEST_PACKAGE)
            .unwrap()
            .into_iter()
            .find(|p| p.name == NOTIFICATIONS)
            .map(|p| p.granted)
    };
    pm.grant(TEST_PACKAGE, NOTIFICATIONS, None).unwrap();
    assert_eq!(granted(&pm), Some(true));
    pm.revoke(TEST_PACKAGE, NOTIFICATIONS, None).unwrap();
    assert_eq!(granted(&pm), Some(false));

    assert!(pm.grant("com.nonexistent", NOTIFICATIONS, None).is_err());
}

#[test]
fn flags() {
    let device = device();
    install_test_app(&device);
    let pm = device.pm();
    assert!(pm
        .package_flags(TEST_PACKAGE)
        .unwrap()
        .contains(&PackageFlags::Debuggable));
    assert!(!pm.is_system(TEST_PACKAGE).unwrap());
    assert!(pm.is_system("com.android.settings").unwrap());
    assert!(pm.package_flags("com.nonexistent").is_err());
}

#[test]
fn enable_and_disable() {
    let device = device();
    install_test_app(&device);
    let pm = device.pm();
    let disabled = |pm: &radb_client::PackageManager| {
        let filter = ListPackageFilter {
            show_only_disabled: true,
            ..Default::default()
        };
        pm.list_packages(filter, ListPackageDisplayOptions::default(), None)
            .unwrap()
            .iter()
            .any(|p| p.package_name == TEST_PACKAGE)
    };

    pm.disable_user(TEST_PACKAGE, None).unwrap();
    assert!(disabled(&pm));
    pm.enable(TEST_PACKAGE, None).unwrap();
    assert!(!disabled(&pm));

    pm.suspend(TEST_PACKAGE, None).unwrap();
    pm.unsuspend(TEST_PACKAGE, None).unwrap();
    pm.clear(TEST_PACKAGE, None).unwrap();
}

/// Install from a device path with a space in the name.
#[test]
fn install_from_device() {
    let device = device();
    let remote = tmp_path("app.apk");
    device.push(TEST_APK, &remote).unwrap();

    let pm = device.pm();
    let keep = UninstallOptions {
        keep_data: true,
        ..Default::default()
    };
    if pm.is_installed(TEST_PACKAGE, None).unwrap() {
        pm.uninstall(TEST_PACKAGE, Some(keep)).unwrap();
    }
    assert!(!pm.is_installed(TEST_PACKAGE, None).unwrap());

    let options = InstallOptions {
        replace_existing_application: true,
        grant_permissions: true,
        ..Default::default()
    };
    pm.install(&remote, Some(options)).unwrap();
    assert!(pm.is_installed(TEST_PACKAGE, None).unwrap());
    device.shell().rm(&remote, &[]).unwrap();
}
