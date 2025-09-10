use std::slice;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, TimeZone, Utc};
use windows::Win32::System::WindowsProgramming::{GetComputerNameA, MAX_COMPUTERNAME_LENGTH};
use windows::core::PSTR;

use crate::error::WindowsError;

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
    let mut name = Vec::with_capacity(length as usize);
    unsafe {
        GetComputerNameA(Some(PSTR::from_raw(name.as_mut_ptr())), &mut length)?;

        let result = slice::from_raw_parts(name.as_ptr(), length as usize);
        Ok(String::from_utf8_lossy(result).to_string())
    }
}
