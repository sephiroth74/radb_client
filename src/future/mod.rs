pub mod thread_pool;

use tokio::sync::Semaphore;

#[allow(dead_code)]
pub struct ThreadPool {
    sem: Semaphore,
}
