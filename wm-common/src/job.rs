use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectA, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
    JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0, JobObjectCpuRateControlInformation,
    SetInformationJobObject,
};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::core::PCSTR;

use crate::error::WindowsError;

pub struct AssignJobGuard {
    _job: HANDLE,
}

impl AssignJobGuard {
    pub fn new(name: &str) -> Result<Self, WindowsError> {
        unsafe {
            let job = CreateJobObjectA(None, PCSTR::from_raw(name.as_ptr()))?;
            let current_process = GetCurrentProcess();
            AssignProcessToJobObject(job, current_process)?;

            Ok(Self { _job: job })
        }
    }

    pub fn cpu_limit(&self, rate: f64) -> Result<(), WindowsError> {
        let control_info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
            ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
                CpuRate: (10000.0 * rate) as u32,
            },
        };
        unsafe {
            SetInformationJobObject(
                self._job,
                JobObjectCpuRateControlInformation,
                &control_info as *const JOBOBJECT_CPU_RATE_CONTROL_INFORMATION as *const c_void,
                size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>()
                    .try_into()
                    .unwrap(),
            )?;
        }
        Ok(())
    }
}

impl Drop for AssignJobGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self._job);
        }
    }
}
