use std::time::{Duration, Instant};

use radb_client::{
    ChmodMode, ChmodModeSpecs, ChmodOptions, DumpsysOptions, Error, InputSource, KeyCode, PropType,
    SELinuxType, ScreenRecordOptions, SettingsType,
};

use crate::common::{device, tmp_path, TMP_DIR};

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The arguments reach the device program unchanged.
#[test]
fn exec_quotes_arguments() {
    let device = device();
    let output = device
        .shell()
        .exec(["printf", "[%s]", "a  b", "$HOME", "it's", "`id`", "*", ""])
        .unwrap();
    assert_eq!(stdout(&output), "[a  b][$HOME][it's][`id`][*][]");
}

#[test]
fn exec_fails_on_error() {
    let device = device();
    let err = device.shell().exec(["cat", "/nonexistent"]).unwrap_err();
    assert_eq!(err.exit_code(), Some(1));
    match &err {
        Error::CommandFailed(failure) => {
            assert!(failure.stderr.contains("No such file"), "{failure:?}");
            assert!(failure.command.contains("cat /nonexistent"), "{failure:?}");
        }
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn exec_script_and_raw() {
    let device = device();
    let shell = device.shell();
    assert_eq!(
        stdout(&shell.exec_script("echo $((1 + 2)) | cat").unwrap()),
        "3\n"
    );
    assert_eq!(
        shell.exec_script("exit 4").unwrap_err().exit_code(),
        Some(4)
    );

    // exec_raw: the device shell parses the joined arguments, and errors are not checked
    let output = shell.exec_raw(["echo", "a  b"], None, None).unwrap();
    assert_eq!(stdout(&output), "a b\n");
    assert!(!shell
        .exec_raw(["false"], None, None)
        .unwrap()
        .status
        .success());
}

#[test]
fn exec_with_timeout_and_cancel() {
    let device = device();
    let shell = device.shell();

    let start = Instant::now();
    shell
        .exec_with(["sleep", "30"], None, Some(Duration::from_secs(1)))
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(10));

    let (tx, rx) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let _ = tx.send(());
    });
    let start = Instant::now();
    shell.exec_with(["sleep", "30"], Some(rx), None).unwrap();
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn spawn() {
    let device = device();
    let mut child = device.shell().spawn(["sleep", "1"]).unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn props() {
    let device = device();
    let shell = device.shell();
    assert!(shell.build_version_sdk().unwrap() >= 24);
    assert!(!shell.build_fingerprint().unwrap().is_empty());
    assert_eq!(shell.getprop("radb.nonexistent").unwrap(), "");
    assert_eq!(
        shell.getprop_type("ro.build.version.sdk").unwrap(),
        PropType::Int
    );

    let props = shell.getprops().unwrap();
    assert!(props.iter().any(|p| p.key == "ro.build.version.sdk"));
    let types = shell.getprops_type().unwrap();
    assert_eq!(types.get("ro.build.version.sdk"), Some(&PropType::Int));

    let value = "hello world 'quoted' $HOME";
    shell.setprop("debug.radb.test", value).unwrap();
    assert_eq!(shell.getprop("debug.radb.test").unwrap(), value);
    shell.clear_prop("debug.radb.test").unwrap();
    assert_eq!(shell.getprop("debug.radb.test").unwrap(), "");
}

#[test]
fn settings() {
    let device = device();
    let shell = device.shell();
    let key = "radb_test_setting";
    let value = "a value; with 'spaces'";

    shell.put_setting(SettingsType::global, key, value).unwrap();
    assert_eq!(
        shell
            .get_setting(SettingsType::global, key)
            .unwrap()
            .as_deref(),
        Some(value)
    );
    assert!(shell
        .list_settings(SettingsType::global)
        .unwrap()
        .iter()
        .any(|p| p.key == key && p.value == value));

    shell.delete_setting(SettingsType::global, key).unwrap();
    assert_eq!(shell.get_setting(SettingsType::global, key).unwrap(), None);
}

#[test]
fn files() {
    let device = device();
    let shell = device.shell();
    assert!(shell.is_dir("/system").unwrap());
    assert!(shell.is_file("/system/build.prop").unwrap());
    assert!(shell.is_symlink("/sdcard").unwrap());
    assert!(!shell.exists("/nonexistent").unwrap());
    assert!(shell
        .ls("/system", &[])
        .unwrap()
        .contains(&"app".to_string()));
    assert!(String::from_utf8(shell.cat("/proc/version").unwrap())
        .unwrap()
        .starts_with("Linux"));
    // not readable by the shell user on recent Android versions: an error, not an empty file
    assert_eq!(
        shell.cat("/system/build.prop").unwrap_err().exit_code(),
        Some(1)
    );
    assert_eq!(
        shell.file_mode("/system/bin/sh").unwrap().mode() & 0o777,
        0o755
    );

    // a path with spaces
    let dir = tmp_path("dir");
    let file = format!("{dir}/a file");
    shell.exec(["mkdir", "-p", &dir]).unwrap();
    shell.exec(["touch", &file]).unwrap();
    assert!(shell.is_file(&file).unwrap());
    assert_eq!(shell.ls(&dir, &[]).unwrap(), ["a file"]);

    let rw = ChmodMode {
        read: true,
        write: true,
        execute: false,
    };
    let r = ChmodMode {
        read: true,
        write: false,
        execute: false,
    };
    let options = ChmodOptions {
        mode: ChmodModeSpecs {
            owner: rw,
            group: r,
            other: ChmodMode::default(),
        },
        ..Default::default()
    };
    shell.chmod(options, &[&file]).unwrap();
    assert_eq!(shell.file_mode(&file).unwrap().mode() & 0o777, 0o640);

    let sizes = shell.dir_size(&dir, true).unwrap();
    assert_eq!(sizes.len(), 1);
    assert_eq!(sizes[0].0, dir);

    shell.rm(&dir, &["-r"]).unwrap();
    assert!(!shell.exists(&dir).unwrap());
    assert!(shell.rm(&dir, &[]).is_err());
    assert!(shell
        .dir_size(format!("{TMP_DIR}/nonexistent"), true)
        .is_err());
}

#[test]
fn which() {
    let device = device();
    let shell = device.shell();
    assert_eq!(
        shell.which("ls").unwrap().as_deref(),
        Some("/system/bin/ls")
    );
    assert_eq!(shell.which("radb-nonexistent").unwrap(), None);
    assert!(shell.has_avbctl().unwrap());
}

#[test]
fn selinux_and_verity() {
    let device = device();
    let shell = device.shell();
    assert_eq!(shell.get_enforce().unwrap(), SELinuxType::Enforcing);
    assert!(
        shell.set_enforce(SELinuxType::Permissive).is_err(),
        "needs root"
    );

    device.root().unwrap();
    shell.set_enforce(SELinuxType::Permissive).unwrap();
    assert_eq!(shell.get_enforce().unwrap(), SELinuxType::Permissive);
    shell.set_enforce(SELinuxType::Enforcing).unwrap();
    assert_eq!(shell.get_enforce().unwrap(), SELinuxType::Enforcing);
    shell.get_verity().unwrap();
    device.unroot().unwrap();
}

#[test]
fn dumpsys() {
    let device = device();
    let shell = device.shell();
    let services = shell.dumpsys_list(false, None).unwrap();
    assert!(services.contains(&"battery".to_string()));
    let dump = shell
        .dumpsys(Some("battery"), &[], DumpsysOptions::default())
        .unwrap();
    assert!(dump.contains("level:"), "{dump}");
    assert!(shell.is_screen_on().is_ok());
    assert!(!shell.get_events().unwrap().is_empty());
}

#[test]
fn input() {
    let device = device();
    let shell = device.shell();
    shell
        .send_keyevent(KeyCode::KEYCODE_WAKEUP, None, None)
        .unwrap();
    shell
        .send_keyevents([KeyCode::KEYCODE_DPAD_DOWN, KeyCode::KEYCODE_DPAD_UP], None)
        .unwrap();
    shell.send_keycode(3, None, None).unwrap(); // KEYCODE_HOME
    shell.send_tap((100, 100), None).unwrap();
    shell
        .send_swipe(
            (100, 800),
            (100, 200),
            Some(Duration::from_millis(100)),
            Some(InputSource::touchscreen),
        )
        .unwrap();
    shell.send_text("hello world it's $HOME", None).unwrap();
    shell.send_char('x', None).unwrap();
    shell
        .send_keyevent(KeyCode::KEYCODE_HOME, None, None)
        .unwrap();
}

#[test]
fn screencap_and_screenrecord() {
    let device = device();
    let shell = device.shell();

    let png = tmp_path("screen.png");
    shell.save_screencap(&png).unwrap();
    assert!(shell.is_file(&png).unwrap());
    shell.rm(&png, &[]).unwrap();

    let mp4 = tmp_path("screen.mp4");
    let options = ScreenRecordOptions {
        timelimit: Some(Duration::from_secs(1)),
        ..Default::default()
    };
    shell.screen_record(Some(options), &mp4, None).unwrap();
    assert!(shell.is_file(&mp4).unwrap());
    shell.rm(&mp4, &[]).unwrap();
}
