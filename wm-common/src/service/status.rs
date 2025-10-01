use windows::Win32::System::Services;
use windows::Win32::System::Services::{SERVICE_STATUS, SERVICE_STATUS_CURRENT_STATE};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServiceState {
    ContinuePending,
    PausePending,
    Paused,
    Running,
    StartPending,
    StopPending,
    Stopped,
}

impl From<SERVICE_STATUS_CURRENT_STATE> for ServiceState {
    fn from(value: SERVICE_STATUS_CURRENT_STATE) -> Self {
        match value {
            Services::SERVICE_CONTINUE_PENDING => Self::ContinuePending,
            Services::SERVICE_PAUSE_PENDING => Self::PausePending,
            Services::SERVICE_PAUSED => Self::Paused,
            Services::SERVICE_RUNNING => Self::Running,
            Services::SERVICE_START_PENDING => Self::StartPending,
            Services::SERVICE_STOP_PENDING => Self::StopPending,
            Services::SERVICE_STOPPED => Self::Stopped,
            _ => panic!("Unknown service state {}", value.0),
        }
    }
}

pub struct ServiceStatus {
    _inner: SERVICE_STATUS,
    pub current_state: ServiceState,
}

impl ServiceStatus {
    pub fn new(status: SERVICE_STATUS) -> Self {
        Self {
            _inner: status,
            current_state: ServiceState::from(status.dwCurrentState),
        }
    }
}
