use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

struct _DropGuard<'a, T> {
    _cell: &'a OnceCellNoRetry<T>,
}

impl<T> Drop for _DropGuard<'_, T> {
    fn drop(&mut self) {
        self._cell._initializing.store(false, Ordering::Release);
        self._cell._waiter.notify_waiters();
    }
}

pub struct OnceCellNoRetry<T> {
    _waiter: Notify,
    _inner: UnsafeCell<MaybeUninit<T>>,
    _initializing: AtomicBool,
    _initialized: AtomicBool,
}

impl<T> OnceCellNoRetry<T> {
    pub fn new() -> Self {
        Self::new_with(None)
    }

    pub fn new_with(value: Option<T>) -> Self {
        let initialized = value.is_some();
        Self {
            _waiter: Notify::new(),
            _inner: UnsafeCell::new(match value {
                Some(v) => MaybeUninit::new(v),
                None => MaybeUninit::uninit(),
            }),
            _initializing: AtomicBool::new(false),
            _initialized: AtomicBool::new(initialized),
        }
    }

    /// The underlying value must not be uninitialized.
    unsafe fn _get_unchecked(&self) -> &T {
        unsafe {
            let init = &*self._inner.get();
            init.assume_init_ref()
        }
    }

    unsafe fn _set_unchecked(&self, value: T) {
        unsafe {
            let init = &mut *self._inner.get();
            init.write(value);
        }
    }

    pub async fn get_or_try_init<E, F, Fut>(&self, f: F) -> Option<&T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        if self._initialized.load(Ordering::Acquire) {
            return Some(unsafe { self._get_unchecked() });
        }

        match self._initializing.compare_exchange(
            false,
            true,
            Ordering::Acquire,
            // We do not care about the inner value of Err(_)
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                let _guard = _DropGuard { _cell: self };
                match f().await {
                    Ok(result) => {
                        unsafe { self._set_unchecked(result) };
                        self._initialized.store(true, Ordering::Release);
                        Some(unsafe { self._get_unchecked() })
                    }
                    Err(_) => None,
                }
            }
            Err(_) => {
                self._waiter.notified().await;
                if self._initialized.load(Ordering::Acquire) {
                    Some(unsafe { self._get_unchecked() })
                } else {
                    None
                }
            }
        }
    }
}

impl<T> Default for OnceCellNoRetry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for OnceCellNoRetry<T> {
    fn drop(&mut self) {
        if self._initialized.load(Ordering::Acquire) {
            unsafe {
                let init = self._inner.get_mut();
                init.assume_init_drop();
            }
        }
    }
}

unsafe impl<T: Send> Send for OnceCellNoRetry<T> {}
unsafe impl<T: Sync> Sync for OnceCellNoRetry<T> {}
