pub mod service_manager;
pub mod status;

use windows::Win32::System::Services;

pub const SC_MANAGER_ALL_ACCESS: u32 = Services::SC_MANAGER_ALL_ACCESS;
