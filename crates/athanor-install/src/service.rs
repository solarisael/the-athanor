#[cfg(windows)]
mod windows {
    use crate::{
        installer::CurrentRelease,
        layout::{InstallLayout, SERVICE_NAME},
        supervisor::{
            NativeProcesses, RuntimeConfig, StartProgress, Supervisor, prepare_service_console,
            runtime_plan,
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
        Foundation::{
            ERROR_CALL_NOT_IMPLEMENTED, ERROR_SERVICE_SPECIFIC_ERROR, ERROR_SUCCESS, GetLastError,
        },
        System::Services::*,
    };

    /// The service-specific code SCM shows when the Athanor failed to start or
    /// stop cleanly. `sc query` then reports `SERVICE_EXIT_CODE : 1066` with
    /// this value, instead of a clean stop.
    pub const SERVICE_FAILURE_CODE: u32 = 1;

    static STOP_SENDER: OnceLock<Mutex<mpsc::Sender<()>>> = OnceLock::new();

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }


    pub fn ensure_running(config: &RuntimeConfig) -> Result<()> {
        use crate::supervisor::{START_PROGRESS_INTERVAL, START_TIMEOUT, loopback_address};
        use std::{net::TcpStream, thread, time::{Duration, Instant}};
        let ports = [
            ("PostgreSQL", loopback_address(&config.database_host, config.database_port)?),
            ("NATS", loopback_address(&config.nats_host, config.nats_port)?),
        ];
        let started = Instant::now();
        let manager = unsafe { OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT) };
        if manager.is_null() {
            bail!("service {SERVICE_NAME}: OpenSCManagerW failed after 0 seconds: {}", unsafe { GetLastError() });
        }
        let name = wide(SERVICE_NAME);
        let service = unsafe { OpenServiceW(manager, name.as_ptr(), SERVICE_QUERY_STATUS) };
        let result = (|| -> Result<()> {
            if service.is_null() {
                bail!("service {SERVICE_NAME}: OpenServiceW failed after 0 seconds: {}", unsafe { GetLastError() });
            }
            let mut next_progress = START_PROGRESS_INTERVAL;
            let mut requested = false;
            loop {
                let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
                if unsafe { QueryServiceStatus(service, &mut status) } == 0 {
                    bail!("service {SERVICE_NAME}: QueryServiceStatus failed after {} seconds: {}", started.elapsed().as_secs(), unsafe { GetLastError() });
                }
                if status.dwCurrentState == SERVICE_STOPPED {
                    if requested {
                        bail!("service {SERVICE_NAME} stopped before readiness after {} seconds (Win32 {}, service {})", started.elapsed().as_secs(), status.dwWin32ExitCode, status.dwServiceSpecificExitCode);
                    }
                    let starter = unsafe { OpenServiceW(manager, name.as_ptr(), SERVICE_START) };
                    if starter.is_null() {
                        bail!("service {SERVICE_NAME}: cannot obtain start access after {} seconds: {}", started.elapsed().as_secs(), unsafe { GetLastError() });
                    }
                    let accepted = unsafe { StartServiceW(starter, 0, ptr::null()) };
                    let error = unsafe { GetLastError() };
                    unsafe { CloseServiceHandle(starter); }
                    if accepted == 0 {
                        if error != windows_sys::Win32::Foundation::ERROR_SERVICE_ALREADY_RUNNING {
                            bail!("service {SERVICE_NAME}: StartServiceW failed after {} seconds: {error}", started.elapsed().as_secs());
                        }
                    }
                    requested = true;
                }
                let ready = ports.map(|(_, address)| TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok());
                if status.dwCurrentState == SERVICE_RUNNING && ready.iter().all(|ready| *ready) {
                    return Ok(());
                }
                let elapsed = started.elapsed();
                if elapsed >= START_TIMEOUT || elapsed >= next_progress {
                    let unavailable: Vec<String> = ports.iter().zip(ready)
                        .filter(|(_, ready)| !ready)
                        .map(|((name, address), _)| format!("{name} port {address}"))
                        .collect();
                    let waiting = if unavailable.is_empty() {
                        format!("service RUNNING state (current {})", status.dwCurrentState)
                    } else {
                        unavailable.join(", ")
                    };
                    if elapsed >= START_TIMEOUT {
                        bail!("service {SERVICE_NAME}: {waiting} not ready after {} seconds", elapsed.as_secs());
                    }
                    eprintln!("athanor: service {SERVICE_NAME}: waiting for {waiting}; {} of {} seconds", elapsed.as_secs(), START_TIMEOUT.as_secs());
                    next_progress += START_PROGRESS_INTERVAL;
                }
                thread::sleep(Duration::from_millis(100));
            }
        })();
        unsafe {
            if !service.is_null() { CloseServiceHandle(service); }
            CloseServiceHandle(manager);
        }
        result
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
        publish_status(handle, state, checkpoint, None)
    }

    /// Report `SERVICE_STOPPED` with a nonzero service-specific code, so SCM
    /// and `sc query` show a failure instead of a clean stop.
    fn set_stopped_failed(handle: SERVICE_STATUS_HANDLE) -> Result<()> {
        publish_status(handle, SERVICE_STOPPED, 0, Some(SERVICE_FAILURE_CODE))
    }

    fn publish_status(
        handle: SERVICE_STATUS_HANDLE,
        state: u32,
        checkpoint: u32,
        failure: Option<u32>,
    ) -> Result<()> {
        let pending = state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING;
        let status = SERVICE_STATUS {
            dwServiceType: SERVICE_WIN32_OWN_PROCESS,
            dwCurrentState: state,
            dwControlsAccepted: if state == SERVICE_RUNNING {
                SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
            } else {
                0
            },
            dwWin32ExitCode: if failure.is_some() {
                ERROR_SERVICE_SPECIFIC_ERROR
            } else {
                ERROR_SUCCESS
            },
            dwServiceSpecificExitCode: failure.unwrap_or(0),
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
        if let Err(error) = &result {
            trace_service_start(&format!("service failed: {error:#}"));
            let _ = set_stopped_failed(handle);
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
            processes: NativeProcesses::with_log_dir(layout.data.join("logs")),
        };
        let mut checkpoint = 1u32;
        supervisor.run(&specs, |name, progress| {
            checkpoint += 1;
            match progress {
                StartProgress::Spawned => {
                    trace_service_start(&format!("managed child spawned: {name}"));
                }
                StartProgress::Waiting => {
                    trace_service_start(&format!("managed child starting: {name}"));
                }
            }
            set_status(handle, SERVICE_START_PENDING, checkpoint)
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
pub use windows::{dispatch, ensure_running};

#[cfg(not(windows))]
pub fn dispatch() -> anyhow::Result<()> {
    anyhow::bail!("the managed service is supported only on Windows")
}

#[cfg(not(windows))]
pub fn ensure_running(_: &crate::supervisor::RuntimeConfig) -> anyhow::Result<()> {
    anyhow::bail!("service {} startup is supported only on Windows", crate::layout::SERVICE_NAME)
}
