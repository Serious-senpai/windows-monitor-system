use std::any::type_name;
use std::mem::size_of;
use std::slice;

use ferrisetw::EventRecord;
use windows::Win32::System::Diagnostics::Etw::EVENT_RECORD;
use wm_common::error::RuntimeError;

pub struct FastParser<'r> {
    _record: &'r EventRecord,
    _buffer: &'r [u8],
}

impl<'r> FastParser<'r> {
    pub fn new(record: &'r EventRecord) -> Self {
        let buffer = unsafe {
            let native = &*(record as *const EventRecord as *const EVENT_RECORD);
            slice::from_raw_parts(native.UserData as *const u8, native.UserDataLength as usize)
        };

        Self {
            _record: record,
            _buffer: buffer,
        }
    }

    pub fn buffer(&self) -> &'r [u8] {
        self._buffer
    }

    pub fn skip(&mut self, offset: usize) {
        self._buffer = &self._buffer[offset..];
    }

    pub fn try_read<T>(&mut self) -> Result<T, RuntimeError>
    where
        Self: private::FastParserImpl<T>,
    {
        use private::FastParserImpl;
        self._try_read_impl()
    }
}

mod private {
    use wm_common::error::RuntimeError;

    pub trait FastParserImpl<T> {
        fn _try_read_impl(&mut self) -> Result<T, RuntimeError>;
    }
}

impl private::FastParserImpl<String> for FastParser<'_> {
    fn _try_read_impl(&mut self) -> Result<String, RuntimeError> {
        let (prefix, aligned, _) = unsafe { self._buffer.align_to::<u16>() };

        if prefix.is_empty() {
            // Properly aligned
            let end = aligned
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(aligned.len());

            let s = String::from_utf16_lossy(&aligned[..end]);
            let (_, buffer, _) = unsafe { aligned[end..].align_to::<u8>() };
            self._buffer = buffer;

            Ok(s)
        } else {
            // Not properly aligned
            let mut buf = Vec::with_capacity(self._buffer.len());
            let mut index = 0;
            for chunk in prefix.chunks_exact(2) {
                index += 2;
                if chunk == [0, 0] {
                    break;
                }

                buf.push(u16::from_le_bytes([chunk[0], chunk[1]]));
            }

            self._buffer = &self._buffer[index..];
            Ok(String::from_utf16_lossy(&buf))
        }
    }
}

macro_rules! _fast_parser_impl {
    ($T:ident) => {
        impl private::FastParserImpl<$T> for FastParser<'_> {
            fn _try_read_impl(&mut self) -> Result<$T, RuntimeError> {
                let (head, tail) = self._buffer.split_at(size_of::<$T>());
                self._buffer = tail;
                Ok($T::from_le_bytes(head.try_into().map_err(|e| {
                    RuntimeError::new(format!("Cannot unpack {}: {e}", type_name::<$T>()))
                })?))
            }
        }
    };
}

_fast_parser_impl!(u8);
_fast_parser_impl!(u16);
_fast_parser_impl!(u32);
_fast_parser_impl!(u64);
_fast_parser_impl!(usize);
_fast_parser_impl!(i8);
_fast_parser_impl!(i16);
_fast_parser_impl!(i32);
_fast_parser_impl!(i64);
_fast_parser_impl!(isize);
