# radb_client

ADB client written for rust.


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