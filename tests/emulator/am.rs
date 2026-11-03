use radb_client::{Error, Intent, MemoryStatus, UserOption};

use crate::common::{device, install_test_app, TEST_ACTIVITY, TEST_PACKAGE};

fn main_activity() -> Intent {
    Intent {
        component: Some(TEST_ACTIVITY.to_string()),
        wait: true,
        ..Default::default()
    }
}

#[test]
fn start_and_stop() {
    let device = device();
    install_test_app(&device);
    let am = device.am();

    let mut intent = main_activity();
    intent
        .extra
        .put_string_extra("title", "a title with 'quotes' and spaces");
    intent
        .extra
        .put_string_array_extra("names", vec!["a,b".to_string(), "c".to_string()]);
    am.start(&intent).unwrap();

    am.trim_memory(TEST_PACKAGE, MemoryStatus::RunningLow)
        .unwrap();
    am.kill(UserOption::None, TEST_PACKAGE).unwrap();
    am.force_stop(TEST_PACKAGE).unwrap();
}

#[test]
fn errors() {
    let device = device();
    let am = device.am();

    let missing = Intent {
        component: Some("com.nonexistent/.Main".to_string()),
        ..Default::default()
    };
    match am.start(&missing).unwrap_err() {
        Error::CommandFailed(failure) => {
            assert!(failure.message().contains("does not exist"), "{failure:?}")
        }
        other => panic!("unexpected error {other:?}"),
    }
    assert!(am.start_service(&missing).is_err());
    assert!(am
        .trim_memory("com.nonexistent", MemoryStatus::RunningLow)
        .is_err());
}

#[test]
fn broadcast() {
    let device = device();
    let mut intent = Intent::from_action("radb.test.ACTION");
    intent.receiver_foreground = true;
    intent
        .extra
        .put_string_extra("value", "a b; c")
        .put_int_extra("count", 3);
    device.am().broadcast(&intent).unwrap();
}

#[test]
fn users_and_processes() {
    let device = device();
    let am = device.am();
    assert_eq!(am.get_current_user().unwrap(), "0");
    am.kill(UserOption::Current, TEST_PACKAGE).unwrap();
    am.kill_all().unwrap();
}

#[test]
fn crash() {
    let device = device();
    install_test_app(&device);
    device.am().start(&main_activity()).unwrap();
    device.am().crash(None, TEST_PACKAGE).unwrap();
    device.am().force_stop(TEST_PACKAGE).unwrap();
}
