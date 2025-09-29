use std::ptr;

pub struct PtrGuard<T> {
    _ptr: *mut T,
    _drop: Box<dyn Fn(*mut T)>,
}

impl<T> PtrGuard<T> {
    pub fn new(on_drop: impl Fn(*mut T) + 'static) -> Self {
        Self {
            _ptr: ptr::null_mut(),
            _drop: Box::new(on_drop),
        }
    }

    pub fn ptr(&mut self) -> *mut T {
        self._ptr
    }
}

impl<T> Drop for PtrGuard<T> {
    fn drop(&mut self) {
        (self._drop)(self._ptr);
    }
}
