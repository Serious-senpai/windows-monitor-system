use std::ffi::{CStr, CString, c_void};
use std::slice;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, TimeZone, Utc};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertStringSidToSidA;
use windows::Win32::Security::PSID;
use windows::Win32::System::WindowsProgramming::{GetComputerNameA, MAX_COMPUTERNAME_LENGTH};
use windows::Win32::UI::Shell::CommandLineToArgvW;
use windows::core::{PCSTR, PCWSTR, PSTR};

use crate::error::WindowsError;
use crate::ptr_guard::PtrGuard;

fn _windows_timestamp<const NSECS: bool>(value: i64) -> DateTime<Utc> {
    static BASE: LazyLock<DateTime<Utc>> =
        LazyLock::new(|| Utc.with_ymd_and_hms(1601, 1, 1, 0, 0, 0).unwrap());

    let secs = value / 10_000_000;
    let nsecs = if NSECS { (value % 10_000_000) * 100 } else { 0 };
    *BASE + Duration::seconds(secs) + Duration::nanoseconds(nsecs)
}

pub fn windows_timestamp(value: i64) -> DateTime<Utc> {
    _windows_timestamp::<true>(value)
}

pub fn windows_timestamp_rounded(value: i64) -> DateTime<Utc> {
    _windows_timestamp::<false>(value)
}

pub fn get_computer_name() -> Result<String, WindowsError> {
    let mut length = MAX_COMPUTERNAME_LENGTH + 1;
    let mut name = vec![0; length as usize];
    unsafe {
        GetComputerNameA(Some(PSTR::from_raw(name.as_mut_ptr())), &mut length)?;

        let result = slice::from_raw_parts(name.as_ptr(), length as usize);
        Ok(String::from_utf8_lossy(result).to_string())
    }
}

pub fn split_command_line(command_line: &str) -> Vec<String> {
    let mut argc = 0;
    let utf16 = command_line
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<u16>>();

    let mut result = vec![];
    unsafe {
        let argv = CommandLineToArgvW(PCWSTR::from_raw(utf16.as_ptr()), &mut argc);

        if !argv.is_null() {
            result.reserve(argc as usize);
            for argument in slice::from_raw_parts(argv, argc as usize) {
                result.push(argument.to_string().unwrap_or_default());
            }

            LocalFree(Some(HLOCAL(argv as *mut c_void)));
        }
    }

    result
}

pub fn convert_sid(stringsid: &CStr) -> Result<PtrGuard<c_void>, WindowsError> {
    let mut sid = PSID::default();
    unsafe {
        ConvertStringSidToSidA(PCSTR::from_raw(stringsid.as_ptr() as *const u8), &mut sid)?;
    }

    let sid = PtrGuard::from_ptr(sid.0, |ptr| unsafe {
        let _ = LocalFree(Some(HLOCAL(ptr)));
    });

    Ok(sid)
}

pub fn to_c_string(s: String) -> CString {
    let bytes = s.into_bytes();
    unsafe { CString::from_vec_unchecked(bytes) }
}
