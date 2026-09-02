#[cfg(windows)]
mod windows {
    use crate::{
        installer::CurrentRelease,
        layout::{InstallLayout, SERVICE_NAME},
        supervisor::{
            NativeProcesses, RuntimeConfig, Supervisor, prepare_service_console, runtime_plan,
        },
    };
    use anyhow::{Context, Result, bail};
    use std::{
        env,
        ffi::c_void,
        fs::{self, OpenOptions},
        io::Write,
        path::PathBuf,
        ptr,
        sync::{Mutex, OnceLock, mpsc},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_CALL_NOT_IMPLEMENTED, ERROR_SUCCESS, GetLastError},
        System::Services::*,
    };

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
            write_service_error(&error);
            eprintln!("Athanor service failed: {error:#}");
        }
    }

    fn service_log_path(name: &str) -> Option<PathBuf> {
        roots()
            .ok()
            .map(|layout| layout.data.join("logs").join(name))
    }

    fn reset_service_trace() {
        let Some(path) = service_log_path("service-startup-trace.log") else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, "");
    }

    fn trace_service_start(message: &str) {
        let Some(path) = service_log_path("service-startup-trace.log") else {
            return;
        };
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{message}");
        }
    }

    fn write_service_error(error: &anyhow::Error) {
        let Some(path) = service_log_path("service-startup-error.log") else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, format!("{error:#}\n{error:?}\n"));
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
        reset_service_trace();
        trace_service_start("control handler registered");
        set_status(handle, SERVICE_START_PENDING, 1)?;
        trace_service_start("start pending reported");
        let result = run_registered(handle);
        if result.is_err() {
            let _ = set_status(handle, SERVICE_STOPPED, 0);
        }
        result
    }

    fn run_registered(handle: SERVICE_STATUS_HANDLE) -> Result<()> {
        let (stop_tx, stop_rx) = mpsc::channel();
        STOP_SENDER
            .set(Mutex::new(stop_tx))
            .map_err(|_| anyhow::anyhow!("service control channel already initialized"))?;
        trace_service_start("stop channel registered");
        prepare_service_console()?;
        trace_service_start("service console ready");

        let layout = roots()?;
        trace_service_start("install roots resolved");
        let current: CurrentRelease = serde_json::from_slice(&fs::read(layout.current())?)?;
        trace_service_start("current release read");
        let config: RuntimeConfig = serde_json::from_slice(&fs::read(layout.config())?)?;
        config.validate()?;
        trace_service_start("runtime config read");
        let specs = runtime_plan(&layout.version(&current.version), &layout.data, &config)?;
        trace_service_start(&format!("runtime plan built: {} children", specs.len()));
        let supervisor = Supervisor {
            processes: NativeProcesses::default(),
        };
        supervisor.run(&specs, |checkpoint, name| {
            trace_service_start(&format!("managed child spawned: {name}"));
            set_status(handle, SERVICE_START_PENDING, checkpoint + 1)
        })?;
        trace_service_start("all managed children ready");
        set_status(handle, SERVICE_RUNNING, 0)?;
        trace_service_start("running reported");
        let _ = fs::remove_file(layout.data.join("logs/service-startup-error.log"));
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
