# radb_client

ADB client written for rust.

[![crates.io](https://img.shields.io/crates/v/radb_client.svg)](https://crates.io/crates/radb_client/)



# Example

```rust

use radb_client::{Adb, AdbClient, Client, Device};

pub fn main() {
    let adb = Adb::new().unwrap();
    let device_ip = String::from("192.168.1.128");
    let device = adb.device(device_ip.as_str()).unwrap();

    // or create an AdbClient directly from the ip address
    // let client: AdbClient = device_ip.as_str().parse::<Device>().unwrap().try_into().unwrap();
    
    match Client::connect(&adb, device.as_ref()) {
        Ok(()) => println!("Device connected"),
        Err(err) => println!("Error connecting to device: {:?}", err),
    }
}

```

Using the feature `scanner` is also possible to scan for all the available devices:

```rust
use radb_client::{Adb, AdbClient, Client, Device};
use radb_client::scanner::{ClientResult, Scanner};
use radb_client::errors::AdbError;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

fn scan_for_devices() -> Result<Vec<ClientResult>, AdbError> {
	info!("Scanning for devices..");

	let adb = Adb::new()?;

	let progress_style = ProgressStyle::with_template(
		"{prefix:.cyan.bold/blue.bold}: {elapsed_precise} [{bar:40.cyan/blue}] {percent:.bold}% {msg} ",
	)
	.unwrap()
	.progress_chars("=> ");

	let multi_progress = MultiProgress::new();
	let progress = multi_progress.add(ProgressBar::new(255));
	progress.set_style(progress_style.clone());
	progress.set_prefix("Scanning");

	let (tx, rx) = crossbeam::channel::bounded(255);

	let scanner = Scanner::new(Duration::from_millis(150), Duration::from_millis(150), false);
	scanner.scan(&adb, tx.clone());

	drop(tx);

	let mut result = Vec::new();
	for client in rx {
		progress.inc(1);

		if let Some(client) = client {
			result.push(client);
		}
	}

	Ok(result)
}
```
