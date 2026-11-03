use std::str::FromStr;
use std::time::Duration;

use cidr_utils::cidr::Ipv4Cidr;
use itertools::Either;
use radb_client::scanner::Scanner;

use crate::common::{adb, device};

/// The emulator adbd also listens on 127.0.0.1:5555.
#[test]
fn scan_localhost() {
    let device = device();
    let model = device.shell().getprop("ro.product.model").unwrap();

    let (tx, rx) = crossbeam_channel::unbounded();
    Scanner::default()
        .with_tcp_timeout(Duration::from_millis(500))
        .with_adb_timeout(Duration::from_secs(5))
        .scan(
            &adb(),
            Ipv4Cidr::from_str("127.0.0.1/32").unwrap().iter(),
            tx,
        );

    let found: Vec<_> = rx
        .iter()
        .filter_map(|message| match message {
            Either::Left(_) => None,
            Either::Right(result) => Some(result),
        })
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].model.as_deref(), Some(model.as_str()));
    assert_eq!(found[0].conn.to_string(), "127.0.0.1:5555");

    // the scanner disconnects the devices that it connected
    let devices = adb().list_devices().unwrap();
    assert!(
        !devices.iter().any(|d| d.serial == "127.0.0.1:5555"),
        "{devices:?}"
    );
}
