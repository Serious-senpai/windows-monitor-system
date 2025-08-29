use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedMutexGuard, mpsc};

pub struct PoolGuard<'a, T> {
    _pool: &'a Pool<T>,
    _mutex: Arc<Mutex<T>>,
    _item: OwnedMutexGuard<T>,
}

impl<T> Deref for PoolGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self._item
    }
}

impl<T> DerefMut for PoolGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self._item
    }
}

impl<T> Drop for PoolGuard<'_, T> {
    fn drop(&mut self) {
        self._pool
            ._sender
            .try_send(self._mutex.clone())
            .expect("Failed to return item to pool");
    }
}

pub struct Pool<T> {
    _sender: mpsc::Sender<Arc<Mutex<T>>>,
    _receiver: Mutex<mpsc::Receiver<Arc<Mutex<T>>>>,
}

impl<T> Pool<T> {
    pub fn new<F>(size: usize, initializer: F) -> Self
    where
        F: Fn(usize) -> T,
    {
        let (sender, receiver) = mpsc::channel(size);
        for i in 0..size {
            let item = Arc::new(Mutex::new(initializer(i)));
            sender.try_send(item).expect("Failed to initialize pool");
        }

        Self {
            _sender: sender,
            _receiver: Mutex::new(receiver),
        }
    }

    pub async fn acquire(&self) -> PoolGuard<'_, T> {
        let mut receiver = self._receiver.lock().await;

        let mutex = receiver.recv().await.expect("Pool channel closed");
        let item = mutex.clone().lock_owned().await;

        PoolGuard {
            _pool: self,
            _mutex: mutex.clone(),
            _item: item,
        }
    }
}
