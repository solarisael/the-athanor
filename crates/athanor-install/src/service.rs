#[cfg(windows)]
mod windows {
    use crate::{
        installer::CurrentRelease,
        layout::{InstallLayout, SERVICE_NAME},
        supervisor::{
            NativeProcesses, Supervisor, SupervisorConfig, prepare_service_console, runtime_plan,
        },
    };
    use anyhow::{Context, Result, bail};
    use serde::Deserialize;
    use std::{
        env,
        ffi::c_void,
        fs,
        path::PathBuf,
        ptr,
        sync::{Mutex, OnceLock, mpsc},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_CALL_NOT_IMPLEMENTED, ERROR_SUCCESS, GetLastError},
        System::Services::*,
    };

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Secrets {
        host_token: String,
        postgres_password: String,
        external_database_url: Option<String>,
    }

    static STOP_SENDER: OnceLock<Mutex<mpsc::Sender<()>>> = OnceLock::new();

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    pub fn dispatch() -> Result<()> {
        let mut name = wide(SERVICE_NAME);
        let table = [
            SERVICE_TABLE_ENTRYW {
                lpServiceName: name.as_mut_ptr(),
                lpServiceProc: Some(service_main),
            },
            SERVICE_TABLE_ENTRYW {
                lpServiceName: ptr::null_mut(),
                lpServiceProc: None,
            },
        ];
        let accepted = unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) };
        if accepted == 0 {
            bail!("StartServiceCtrlDispatcherW failed with {}", unsafe {
                GetLastError()
            });
        }
        Ok(())
    }

    unsafe extern "system" fn control_handler(
        control: u32,
        _: u32,
        _: *mut c_void,
        _: *mut c_void,
    ) -> u32 {
        match control {
            SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
                if let Some(sender) = STOP_SENDER.get() {
                    let _ = sender.lock().unwrap().send(());
                }
                ERROR_SUCCESS
            }
            SERVICE_CONTROL_INTERROGATE => ERROR_SUCCESS,
            _ => ERROR_CALL_NOT_IMPLEMENTED,
        }
    }

    unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
        if let Err(error) = run() {
            eprintln!("Athanor service failed: {error:#}");
        }
    }

    fn set_status(handle: SERVICE_STATUS_HANDLE, state: u32, checkpoint: u32) -> Result<()> {
        let pending = state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING;
        let status = SERVICE_STATUS {
            dwServiceType: SERVICE_WIN32_OWN_PROCESS,
            dwCurrentState: state,
            dwControlsAccepted: if state == SERVICE_RUNNING {
                SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
            } else {
                0
            },
            dwWin32ExitCode: ERROR_SUCCESS,
            dwServiceSpecificExitCode: 0,
            dwCheckPoint: checkpoint,
            dwWaitHint: if pending { 30_000 } else { 0 },
        };
        if unsafe { SetServiceStatus(handle, &status) } == 0 {
            bail!("SetServiceStatus failed with {}", unsafe { GetLastError() });
        }
        Ok(())
    }

    fn roots() -> Result<InstallLayout> {
        let program_files =
            PathBuf::from(env::var_os("ProgramFiles").context("ProgramFiles is unavailable")?);
        let program_data =
            PathBuf::from(env::var_os("ProgramData").context("ProgramData is unavailable")?);
        Ok(InstallLayout::new(&program_files, &program_data))
    }

    fn run() -> Result<()> {
        let name = wide(SERVICE_NAME);
        let handle = unsafe {
            RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), ptr::null_mut())
        };
        if handle.is_null() {
            bail!("RegisterServiceCtrlHandlerExW failed with {}", unsafe {
                GetLastError()
            });
        }
        set_status(handle, SERVICE_START_PENDING, 1)?;
        let (stop_tx, stop_rx) = mpsc::channel();
        STOP_SENDER
            .set(Mutex::new(stop_tx))
            .map_err(|_| anyhow::anyhow!("service control channel already initialized"))?;
        prepare_service_console()?;

        let layout = roots()?;
        let current: CurrentRelease = serde_json::from_slice(&fs::read(layout.current())?)?;
        let config: SupervisorConfig = serde_json::from_slice(&fs::read(layout.config())?)?;
        let secrets: Secrets = serde_json::from_slice(&fs::read(layout.secrets())?)?;
        let database_url = secrets.external_database_url.unwrap_or_else(|| {
            format!(
                "postgresql://athanor:{}@127.0.0.1:5432/athanor",
                secrets.postgres_password
            )
        });
        let specs = runtime_plan(
            &layout.version(&current.version),
            &layout.data,
            &config,
            &database_url,
            &secrets.host_token,
        )?;
        let supervisor = Supervisor {
            processes: NativeProcesses::default(),
        };
        supervisor.run(&specs, |checkpoint, _| {
            set_status(handle, SERVICE_START_PENDING, checkpoint + 1)
        })?;
        set_status(handle, SERVICE_RUNNING, 0)?;
        stop_rx
            .recv()
            .context("service stop channel disconnected")?;
        set_status(handle, SERVICE_STOP_PENDING, 1)?;
        supervisor.stop(&specs)?;
        set_status(handle, SERVICE_STOPPED, 0)
    }
}

#[cfg(windows)]
pub use windows::dispatch;

#[cfg(not(windows))]
pub fn dispatch() -> anyhow::Result<()> {
    anyhow::bail!("the managed service is supported only on Windows")
}
