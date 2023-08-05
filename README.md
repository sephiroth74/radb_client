# radb_client

ADB client written for rust.

[![crates.io](https://img.shields.io/crates/v/radb_client.svg)](https://crates.io/crates/radb_client/)



# Example

```
let adb = Adb::new().unwrap();
let device_ip = String::from("192.168.1.128");
let device = ADB.device(device_ip.as_str()).unwrap();

match Client::connect(&adb, device.as_ref()).await {
    Ok(()) => println!("Device connected"),
    Err(err) => println!("Error connecting to device: {:?}", err),
}

```
