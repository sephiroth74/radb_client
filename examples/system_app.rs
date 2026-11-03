//! Install, update or remove a system app.
//!
//! ```sh
//! cargo run --example system_app -- <ip[:port] | serial> info <package>
//! cargo run --example system_app -- <ip[:port] | serial> install <apk> [--force] [--wipe-data] [--allowlist] [--dir <dir>]
//! cargo run --example system_app -- <ip[:port] | serial> remove <package>
//! ```
//!
//! - `--force`: always replace the system copy ([`SystemAppStrategy::ReplaceSystemCopy`]),
//!   instead of installing an update when possible.
//! - `--wipe-data`: clear the app data after the installation.
//! - `--allowlist`: grant the privileged permissions that the privapp-permissions
//!   allowlist does not grant, instead of failing.
//! - `--dir`: the directory of the system copy, e.g. `/system/priv-app/MyApp`.
//!
//! Set `RUST_LOG=radb_client=debug,simple_cmd=trace` to see every adb command.

use std::process::ExitCode;

use radb_client::{
    Client, ConnectionType, PrivappPermissionsPolicy, SystemAppInstallOptions,
    SystemAppRemoveOptions, SystemAppStrategy,
};

fn usage() -> ExitCode {
    eprintln!(
        "usage: system_app <ip[:port] | serial> info <package>\n       \
         system_app <ip[:port] | serial> install <apk> [--force] [--wipe-data] [--allowlist] [--dir <dir>]\n       \
         system_app <ip[:port] | serial> remove <package>"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("radb_client=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(device), Some(command), Some(target)) = (args.first(), args.get(1), args.get(2))
    else {
        return usage();
    };
    let flags = &args[3..];

    match run(device, command, target, flags) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => usage(),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(device: &str, command: &str, target: &str, flags: &[String]) -> radb_client::Result<bool> {
    // an ip address, or a serial (emulator-5554, a USB serial...)
    let addr =
        ConnectionType::try_from_ip(device).unwrap_or_else(|_| ConnectionType::serial(device));
    let client = Client::try_from(addr)?.with_debug(true);
    if matches!(client.addr, ConnectionType::TcpIp(_)) {
        client.connect(Some(std::time::Duration::from_secs(10)))?;
    }

    match command {
        "info" => {
            let info = client.pm().system_app_info(target)?;
            println!("{info:#?}");
        }
        "install" => {
            let mut options = SystemAppInstallOptions::default();
            let mut flags = flags.iter();
            while let Some(flag) = flags.next() {
                match flag.as_str() {
                    "--force" => options.strategy = SystemAppStrategy::ReplaceSystemCopy,
                    "--wipe-data" => options.wipe_data = true,
                    "--allowlist" => {
                        options.privapp_permissions = PrivappPermissionsPolicy::Allowlist
                    }
                    "--dir" => options.target_dir = flags.next().cloned(),
                    _ => return Ok(false),
                }
            }
            let report = client.install_system_app(target, options)?;
            println!("{:?}: {}", report.method, report.after.active.code_path);
            println!("{report:#?}");
        }
        "remove" => {
            client.remove_system_app(target, SystemAppRemoveOptions::default())?;
            println!("{target} removed");
        }
        _ => return Ok(false),
    }
    Ok(true)
}
