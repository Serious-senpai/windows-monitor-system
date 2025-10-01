use windows::Win32::System::Services::{ENUM_SERVICE_TYPE, SERVICE_STATUS_PROCESS};
use windows::Win32::System::{Services, SystemServices};

use crate::service::status::ServiceState;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServiceType {
    FileSystemDriver,
    KernelDriver,
    OwnProcess,
    ShareProcess,
    InteractiveProcess,
}

const _SERVICE_INTERACTIVE_PROCESS: ENUM_SERVICE_TYPE =
    ENUM_SERVICE_TYPE(SystemServices::SERVICE_INTERACTIVE_PROCESS);

impl From<ENUM_SERVICE_TYPE> for ServiceType {
    fn from(value: ENUM_SERVICE_TYPE) -> Self {
        match value {
            Services::SERVICE_FILE_SYSTEM_DRIVER => Self::FileSystemDriver,
            Services::SERVICE_KERNEL_DRIVER => Self::KernelDriver,
            Services::SERVICE_WIN32_OWN_PROCESS => Self::OwnProcess,
            Services::SERVICE_WIN32_SHARE_PROCESS => Self::ShareProcess,
            _SERVICE_INTERACTIVE_PROCESS => Self::InteractiveProcess,
            _ => panic!("Unknown service type {value:?}"),
        }
    }
}

#[derive(Debug)]
pub struct ServiceStatusProcess {
    _inner: SERVICE_STATUS_PROCESS,

    pub service_type: ServiceType,
    pub current_state: ServiceState,
    pub process_id: u32,
}

impl ServiceStatusProcess {
    pub fn new(status: SERVICE_STATUS_PROCESS) -> Self {
        Self {
            _inner: status,
            service_type: ServiceType::from(status.dwServiceType),
            current_state: ServiceState::from(status.dwCurrentState),
            process_id: status.dwProcessId,
        }
    }
}
