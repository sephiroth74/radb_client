# radb_client

Simple Android **adb** client for rust.

[![crates.io](https://img.shields.io/crates/v/radb_client.svg)](https://crates.io/crates/radb_client/)


# Example

```rust

use radb_client::{*};

pub fn main() {
    let adb = Adb::new().unwrap();
    let device_ip = String::from("192.168.1.128");
    let conn = ConnectionType::try_from_ip(device_ip).unwrap();
    let client = Client::try_from(conn).unwrap();

    match client.connect() {
        Ok(()) => println!("Device connected"),
        Err(err) => println!("Error connecting to device: {:?}", err),
    }
}

```

Using the feature `scanner` is also possible to scan for all the available devices:

```rust
use std::str::FromStr;
use std::time::{Duration, Instant};

use cidr_utils::cidr::Ipv4Cidr;
use cidr_utils::Ipv4CidrSize;
use crossbeam_channel::unbounded;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Either;

use crate::scanner::Scanner;
use crate::test::test::init_log;
use crate::types::Adb;

fn test_scan() {
    let cidr = Ipv4Cidr::from_str("192.168.1.0/24").unwrap();
    let progress_style = ProgressStyle::with_template(
        "{prefix:.cyan.bold/blue.bold}: {elapsed_precise} [{bar:40.cyan/blue}] {percent:.bold}%. {msg} ",
    )
        .unwrap()
        .progress_chars("=> ");

    let multi_progress = MultiProgress::new();
    let progress = multi_progress.add(ProgressBar::new(cidr.size()));
    progress.set_style(progress_style.clone());
    progress.set_prefix("Scanning");

    let (tx, rx) = unbounded();
    let adb = Adb::new().expect("failed to find adb");

    let scanner = Scanner::default()
        .with_debug(false)
        .with_tcp_timeout(Duration::from_millis(200))
        .with_adb_timeout(Duration::from_millis(100));

    let start = Instant::now();
    scanner.scan(&adb, cidr.iter(), tx.clone());

    drop(tx);

    let mut result = Vec::new();
    for either in rx {
        match either {
            Either::Left(addr) => {
                progress.inc(1);
                progress.set_message(format!("{addr}..."));
            }
            Either::Right(client) => {
                if !result.contains(&client) {
                    result.push(client);
                }
            }
        }
    }
    
    progress.finish_with_message(format!("Scanned {} IPs", cidr.size()));

    let elapsed = start.elapsed();

    println!("Time elapsed for scanning is: {:?}ms", elapsed.as_millis());
    println!("Found {:} devices", result.len());

    result.sort_by_key(|k| k.conn);

    for device in result.iter() {
        println!("{device}");
    }
}
```
