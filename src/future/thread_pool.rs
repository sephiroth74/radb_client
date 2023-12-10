use std::future::Future;

use tokio::sync::Semaphore;

use crate::future::ThreadPool;

#[allow(dead_code)]
impl ThreadPool {
	pub fn new(size: usize) -> Self {
		ThreadPool { sem: Semaphore::new(size) }
	}

	pub async fn spawn<T>(&self, f: T) -> T::Output
	where
		T: Future + Send + 'static,
		T::Output: Send + 'static,
	{
		let _handle = self.sem.acquire().await;
		f.await
	}
}

impl Default for ThreadPool {
	fn default() -> Self {
		ThreadPool::new(num_cpus::get() * 2)
	}
}
