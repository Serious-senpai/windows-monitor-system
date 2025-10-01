use std::ptr;

pub struct PtrGuard<T> {
    _ptr: *mut T,
    _drop: Box<dyn Fn(*mut T)>,
}

impl<T> PtrGuard<T> {
    pub fn new(on_drop: impl Fn(*mut T) + 'static) -> Self {
        Self::from_ptr(ptr::null_mut(), on_drop)
    }

    pub fn from_ptr(ptr: *mut T, on_drop: impl Fn(*mut T) + 'static) -> Self {
        Self {
            _ptr: ptr,
            _drop: Box::new(on_drop),
        }
    }

    pub fn as_ptr(&self) -> *const T {
        self._ptr
    }

    pub fn as_mut_ptr(&mut self) -> &mut *mut T {
        &mut self._ptr
    }
}

impl<T> Drop for PtrGuard<T> {
    fn drop(&mut self) {
        (self._drop)(self._ptr);
    }
}
