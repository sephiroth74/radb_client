//! Scan a network range for adb devices (feature `scanner`).

use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use cidr_utils::cidr::InetIterator;
use crossbeam_channel::Sender;
use itertools::Either;
use mac_address::MacAddress;
use tracing::{debug, trace};

use crate::adb::{Adb, AdbDevice};
use crate::client::Client;
use crate::connection::{ConnectionType, DEFAULT_ADB_PORT};
use crate::error::{Error, Result};

const TCP_TIMEOUT: Duration = Duration::from_millis(200);
const ADB_TIMEOUT: Duration = Duration::from_millis(100);

/// Scan addresses for devices listening on the adb port (5555).
///
/// ```no_run
/// use std::str::FromStr;
/// use cidr_utils::cidr::Ipv4Cidr;
/// use itertools::Either;
/// use radb_client::scanner::Scanner;
/// use radb_client::Adb;
///
/// let cidr = Ipv4Cidr::from_str("192.168.1.0/24").unwrap();
/// let (tx, rx) = crossbeam_channel::unbounded();
/// Scanner::default().scan(&Adb::new().unwrap(), cidr.iter(), tx);
/// for message in rx {
///     match message {
///         Either::Left(addr) => println!("scanning {addr}..."),
///         Either::Right(device) => println!("found {device}"),
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Scanner {
    tcp_timeout: Duration,
    adb_timeout: Duration,
    debug: bool,
}

/// A device found by the [`Scanner`].
#[derive(Debug, Clone)]
pub struct ClientResult {
    pub conn: ConnectionType,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    /// SDK version.
    pub version: Option<String>,
    pub mac: Option<MacAddress>,
}

impl Default for Scanner {
    fn default() -> Self {
        Scanner::new(TCP_TIMEOUT, ADB_TIMEOUT, false)
    }
}

impl Scanner {
    pub fn new(tcp_timeout: Duration, adb_timeout: Duration, debug: bool) -> Scanner {
        Scanner {
            tcp_timeout,
            adb_timeout,
            debug,
        }
    }

    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Timeout of the tcp connection to the adb port.
    pub fn with_tcp_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_timeout = timeout;
        self
    }

    /// Timeout of `adb connect`.
    pub fn with_adb_timeout(mut self, timeout: Duration) -> Self {
        self.adb_timeout = timeout;
        self
    }

    /// Scan the addresses in background threads. For every address, `tx` receives
    /// `Either::Left(address)`, then `Either::Right(device)` if a device answers.
    /// The scan is over when all the senders are dropped (drop yours after this call).
    ///
    /// Devices that were not connected before the scan are disconnected again.
    pub fn scan<I>(&self, adb: &Adb, iterator: I, tx: Sender<Either<String, ClientResult>>)
    where
        I: Into<InetIterator<Ipv4Addr>>,
    {
        let adb = Arc::new(adb.clone());
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let pool = threadpool::ThreadPool::new(threads);
        let scanner = self.clone();

        if self.debug {
            debug!("scanning with {threads} threads, {self:?}");
        }

        for ip in iterator.into() {
            let adb = Arc::clone(&adb);
            let tx = tx.clone();
            let scanner = scanner.clone();

            pool.execute(move || {
                let addr = SocketAddr::new(ip.address().into(), DEFAULT_ADB_PORT);
                let _ = tx.send(Either::Left(addr.to_string()));
                if let Some(result) = scanner.probe(&adb, addr) {
                    let _ = tx.send(Either::Right(result));
                }
            });
        }
    }

    fn probe(&self, adb: &Adb, addr: SocketAddr) -> Option<ClientResult> {
        if let Err(err) = TcpStream::connect_timeout(&addr, self.tcp_timeout) {
            if self.debug {
                trace!("[{addr}] tcp connection failed: {err}");
            }
            return None;
        }

        let client = Client::new(adb.clone(), ConnectionType::TcpIp(addr), self.debug);
        let was_connected = client.is_connected();
        if !was_connected && client.connect(Some(self.adb_timeout)).is_err() {
            if self.debug {
                debug!("[{addr}] adb connection failed");
            }
            return Some(ClientResult::new(addr));
        }

        let shell = client.shell();
        let prop = |key: &str| shell.getprop(key).ok().filter(|v| !v.is_empty());
        let result = ClientResult {
            conn: ConnectionType::TcpIp(addr),
            product: prop("persist.sys.stb.name").or_else(|| prop("ro.product.name")),
            model: prop("ro.product.model"),
            device: prop("ro.product.device"),
            version: prop("ro.build.version.sdk"),
            mac: client.get_mac_address().ok(),
        };

        if !was_connected {
            let _ = client.disconnect();
        }
        Some(result)
    }
}

impl ClientResult {
    pub fn new(addr: SocketAddr) -> ClientResult {
        ClientResult {
            conn: ConnectionType::TcpIp(addr),
            product: None,
            model: None,
            device: None,
            version: None,
            mac: None,
        }
    }
}

impl Display for ClientResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}    device", self.conn)?;
        if let Some(product) = &self.product {
            write!(f, " product:{product}")?;
        }
        if let Some(model) = &self.model {
            write!(f, " model:{model}")?;
        }
        if let Some(device) = &self.device {
            write!(f, " device:{device}")?;
        }
        if let Some(mac) = &self.mac {
            write!(f, " mac:{mac}")?;
        }
        if let Some(version) = &self.version {
            write!(f, " version:{version}")?;
        }
        Ok(())
    }
}

impl TryFrom<&ClientResult> for Client {
    type Error = Error;

    fn try_from(value: &ClientResult) -> Result<Self> {
        Client::try_from(value.conn.clone())
    }
}

impl From<&AdbDevice> for ClientResult {
    fn from(value: &AdbDevice) -> Self {
        ClientResult {
            conn: value.addr.clone(),
            product: value.product.clone(),
            model: value.model.clone(),
            device: value.device.clone(),
            version: None,
            mac: None,
        }
    }
}

impl PartialEq for ClientResult {
    fn eq(&self, other: &Self) -> bool {
        self.conn == other.conn
    }
}
