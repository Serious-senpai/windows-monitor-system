use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

use crate::error::WindowsError;
use crate::schema::sysinfo::MemoryInfo;

fn _filetime_to_u64(ft: &FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

pub fn get_system_times() -> Result<(u64, u64, u64), WindowsError> {
    let mut idle_time = FILETIME::default();
    let mut kernel_time = FILETIME::default();
    let mut user_time = FILETIME::default();

    if let Err(e) = unsafe {
        GetSystemTimes(
            Some(&mut idle_time),
            Some(&mut kernel_time),
            Some(&mut user_time),
        )
    } {
        Err(WindowsError::from(e))?;
    }

    Ok((
        _filetime_to_u64(&idle_time),
        _filetime_to_u64(&kernel_time),
        _filetime_to_u64(&user_time),
    ))
}

pub fn memory_status() -> Result<MemoryInfo, WindowsError> {
    let mut status = MEMORYSTATUSEX::default();
    if let Err(e) = unsafe { GlobalMemoryStatusEx(&mut status) } {
        Err(WindowsError::from(e))?;
    }

    Ok(MemoryInfo {
        memory_load: status.dwMemoryLoad,
        total_physical: status.ullTotalPhys,
        available_physical: status.ullAvailPhys,
        total_page_file: status.ullTotalPageFile,
        available_page_file: status.ullAvailPageFile,
        total_virtual: status.ullTotalVirtual,
        available_virtual: status.ullAvailVirtual,
    })
}
