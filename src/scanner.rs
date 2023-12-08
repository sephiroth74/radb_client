use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use mac_address::MacAddress;
use tokio::sync::Semaphore;

use crate::{Adb, Client};

struct Pool {
    sem: Semaphore,
}

impl Pool {
    fn new(size: usize) -> Self {
        Pool { sem: Semaphore::new(size), }
    }

    async fn spawn<T>(&self, f: T) -> T::Output
        where
            T: Future + Send + 'static,
            T::Output: Send + 'static,
    {
        let handle = self.sem.acquire().await;
        f.await
    }
}

impl Default for Pool {
    fn default() -> Self {
        Pool::new(num_cpus::get() * 2)
    }
}

pub struct Scanner {}

impl Scanner {
    pub fn new() -> Scanner {
        Scanner {}
    }

    pub async fn scan2(&self) -> Vec<ClientResult> {
        let adb = Arc::new(Adb::new().unwrap());
        //let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
        let pool = Pool::new(4);

        for i in 0..256 {
            let adb = Arc::clone(&adb);
            pool.spawn(connect(adb, format!("192.168.1.{:}:5555", i))).await;
            //let result = connect(adb, format!("192.168.1.{:}:5555", i)).await;
        }
        Vec::new()
    }

    pub async fn scan(&self) -> Vec<ClientResult> {
        //let outputs: Arc<Mutex<Vec<ClientResult>>> = Arc::new(Mutex::new(Vec::new()));
        let mut outputs = Vec::new();
        let adb = Arc::new(Adb::new().unwrap());
        //adb.root().await.unwrap();

        let mut tasks = vec![];

        for i in 0..256 {
            let adb = Arc::clone(&adb);
            //let outputs = Arc::clone(&outputs);
            let task = tokio::spawn(connect(adb, format!("192.168.1.{:}:5555", i)));

            tasks.push(task);
            //if let Some(client) = task.unwrap().await {
            //    outputs.push(client);
            //}
        }

        join_all(tasks).await;

        //println!("Launched {} tasks...", tasks.len());
        //for task in tasks {
        //    let result = task.await.expect("task failed");
        //
        //    if let Some(client) = result {
        //        println!("Task completed with result: {:?}", &client);
        //        outputs.push(client);
        //    }
        //}

        println!("Ready!");
        println!("{:#?}", outputs);
        outputs

        //let pool = ThreadPool::new();
        //let (tx, rx) = oneshot::channel();
        //
        //pool.spawn(lazy(|_| {
        //    println!("Running on the pool");
        //    tx.send("complete").map_err(|e| println!("send error, {}", e))
        //}));
        //
        //println!("Result: {:?}", rx.wait());
        //pool.shutdown().wait().unwrap();
        //
        //

        //
        //
        //for i in 0..256 {
        //    let adb = Arc::clone(&adb);
        //    let mut outputs = Arc::clone(&outputs);
        //    tokio::spawn(async move {
        //        if let Some(c) = connect(adb, format!("192.168.1.{:}:5555", i)).await {
        //            (*outputs).push(c);
        //            //outputs.deref_mut().push(c);
        //        }
        //    });
        //}

        //
        //let mut tasks = Vec::with_capacity(256);
        //for i in 0..256 {
        //    let adb = Arc::clone(&adb);
        //    tasks.push(tokio::spawn(async move {
        //        connect(adb, format!("192.168.1.{:}:5555", i)).await
        //    }));
        //}
        //
        //
        //for task in tasks {
        //    match task.await {
        //        Ok(addr_opt) => {
        //            if let Some(addr) = addr_opt {
        //                outputs.push(addr)
        //            }
        //        }
        //        Err(_) => {}
        //    }
        //}
        //let result = outputs.lock().unwrap();
        //result.clone()
    }
}

async fn connect(adb: Arc<Adb>, host: String) -> Option<ClientResult> {
    println!("Connecting to {}...", host);

    if let Ok(response) = tokio::time::timeout(Duration::from_millis(50), tokio::net::TcpStream::connect(host.as_str())).await {
        if let Ok(stream) = response {
            if let Ok(addr) = stream.peer_addr() {
                let device = adb.device(host.as_str()).unwrap();
                let _connected = Client::connect(&adb, device.as_ref(), Some(Duration::from_millis(400))).await;
                let client_name = Client::name(&adb, device.as_ref()).await;
                //let client_mac = Client::get_mac_address(&adb, device.as_ref()).await;
                let version = Client::version(&adb, device.as_ref()).await;

                println!("name: {:?}", client_name);

                let _ = Client::disconnect(&adb, device.as_ref()).await;

                Some(ClientResult {
                    addr,
                    name: client_name.unwrap_or(None),
                    mac: None,
                    version: None,
                })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientResult {
    pub addr: SocketAddr,
    pub name: Option<String>,
    pub mac: Option<MacAddress>,
    pub version: Option<u8>,
}
