use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    process::ExitStatus,
};

/// Starts one direct child in its own interactive console on Windows.
pub struct InteractiveCommand {
    program: OsString,
    arguments: Vec<OsString>,
    current_dir: Option<PathBuf>,
    environment: Vec<(OsString, Option<OsString>)>,
}

impl InteractiveCommand {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            arguments: Vec::new(),
            current_dir: None,
            environment: Vec::new(),
        }
    }

    pub fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.arguments.push(argument.as_ref().to_owned());
        self
    }

    pub fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.arguments.extend(
            arguments
                .into_iter()
                .map(|argument| argument.as_ref().to_owned()),
        );
        self
    }

    pub fn current_dir(&mut self, directory: impl AsRef<Path>) -> &mut Self {
        self.current_dir = Some(directory.as_ref().to_owned());
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.environment
            .push((key.as_ref().to_owned(), Some(value.as_ref().to_owned())));
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.environment.push((key.as_ref().to_owned(), None));
        self
    }

    pub fn spawn(&self) -> io::Result<InteractiveChild> {
        platform::spawn(self, true)
    }
}

/// Starts one direct child without creating a console and owns its process tree.
pub struct JobOwnedCommand {
    command: InteractiveCommand,
}

impl JobOwnedCommand {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            command: InteractiveCommand::new(program),
        }
    }

    pub fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(argument);
        self
    }

    pub fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(arguments);
        self
    }

    pub fn current_dir(&mut self, directory: impl AsRef<Path>) -> &mut Self {
        self.command.current_dir(directory);
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.command.env(key, value);
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.command.env_remove(key);
        self
    }

    pub fn spawn(&self) -> io::Result<InteractiveChild> {
        platform::spawn(&self.command, false)
    }
}

/// Owns the native process and kill-on-close job handles until this value drops.
pub struct InteractiveChild {
    inner: platform::Child,
}

impl InteractiveChild {
    pub fn id(&self) -> u32 {
        self.inner.id()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.inner.try_wait()
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.inner.wait()
    }

    pub fn terminate(&mut self) -> io::Result<()> {
        self.inner.terminate()
    }
}

#[cfg(windows)]
mod platform {
    use super::{InteractiveChild, InteractiveCommand};
    use std::{
        cmp::Ordering,
        ffi::{OsStr, OsString, c_void},
        io,
        mem::{size_of, zeroed},
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
            process::ExitStatusExt,
        },
        process::ExitStatus,
        ptr::null,
    };
    use windows_sys::Win32::{
        Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::{
                CREATE_NEW_CONSOLE, CREATE_NEW_PROCESS_GROUP, CREATE_SUSPENDED,
                CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess, INFINITE,
                PROCESS_INFORMATION, ResumeThread, STARTUPINFOW, TerminateProcess,
                WaitForSingleObject,
            },
        },
    };

    const BACKSLASH: u16 = b'\\' as u16;
    const DOUBLE_QUOTE: u16 = b'"' as u16;
    const EQUALS: u16 = b'=' as u16;
    const SPACE: u16 = b' ' as u16;
    const TAB: u16 = b'\t' as u16;
    const TERMINATED_EXIT_CODE: u32 = 1;

    pub(super) struct Child {
        _job: OwnedHandle,
        handle: OwnedHandle,
        pid: u32,
        status: Option<ExitStatus>,
    }

    impl Child {
        pub(super) fn id(&self) -> u32 {
            self.pid
        }

        pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            if let Some(status) = self.status {
                return Ok(Some(status));
            }
            match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => self.read_exit_status().map(Some),
                WAIT_FAILED => Err(io::Error::last_os_error()),
                result => Err(io::Error::other(format!(
                    "unexpected process wait result {result}"
                ))),
            }
        }

        pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
            if let Some(status) = self.status {
                return Ok(status);
            }
            match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), INFINITE) } {
                WAIT_OBJECT_0 => self.read_exit_status(),
                WAIT_FAILED => Err(io::Error::last_os_error()),
                result => Err(io::Error::other(format!(
                    "unexpected process wait result {result}"
                ))),
            }
        }

        pub(super) fn terminate(&mut self) -> io::Result<()> {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            if unsafe { TerminateProcess(self.handle.as_raw_handle(), TERMINATED_EXIT_CODE) } != 0 {
                return Ok(());
            }
            if self.try_wait()?.is_some() {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }

        fn read_exit_status(&mut self) -> io::Result<ExitStatus> {
            let mut code = 0;
            if unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let status = ExitStatus::from_raw(code);
            self.status = Some(status);
            Ok(status)
        }
    }

    fn create_kill_on_close_job() -> io::Result<OwnedHandle> {
        let raw_job = unsafe { CreateJobObjectW(null(), null()) };
        if raw_job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    fn terminate_and_reap_suspended(process: &OwnedHandle) {
        unsafe {
            let _ = TerminateProcess(process.as_raw_handle(), TERMINATED_EXIT_CODE);
            let _ = WaitForSingleObject(process.as_raw_handle(), INFINITE);
        }
    }

    pub(super) fn spawn(
        command: &InteractiveCommand,
        create_new_console: bool,
    ) -> io::Result<InteractiveChild> {
        let application = nul_terminated(&command.program, "program")?;
        let mut command_line = command_line(command)?;
        let mut environment = environment_block(command)?;
        let current_dir = command
            .current_dir
            .as_deref()
            .map(|directory| nul_terminated(directory.as_os_str(), "current directory"))
            .transpose()?;
        let current_dir_pointer = current_dir
            .as_ref()
            .map_or(null(), |directory| directory.as_ptr());
        let job = create_kill_on_close_job()?;
        let mut startup: STARTUPINFOW = unsafe { zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
        let creation_flags = CREATE_NEW_PROCESS_GROUP
            | CREATE_SUSPENDED
            | CREATE_UNICODE_ENVIRONMENT
            | if create_new_console {
                CREATE_NEW_CONSOLE
            } else {
                0
            };
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                0,
                creation_flags,
                environment.as_mut_ptr().cast::<c_void>(),
                current_dir_pointer,
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(io::Error::last_os_error());
        }
        let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
        let thread_handle = unsafe { OwnedHandle::from_raw_handle(process.hThread) };

        if unsafe { AssignProcessToJobObject(job.as_raw_handle(), process_handle.as_raw_handle()) }
            == 0
        {
            let error = io::Error::last_os_error();
            terminate_and_reap_suspended(&process_handle);
            return Err(error);
        }
        if unsafe { ResumeThread(thread_handle.as_raw_handle()) } == u32::MAX {
            let error = io::Error::last_os_error();
            terminate_and_reap_suspended(&process_handle);
            return Err(error);
        }
        drop(thread_handle);

        Ok(InteractiveChild {
            inner: Child {
                _job: job,
                handle: process_handle,
                pid: process.dwProcessId,
                status: None,
            },
        })
    }

    fn command_line(command: &InteractiveCommand) -> io::Result<Vec<u16>> {
        let mut line = Vec::new();
        append_quoted(&mut line, &wide(&command.program, "program")?);
        for argument in &command.arguments {
            line.push(SPACE);
            append_quoted(&mut line, &wide(argument, "argument")?);
        }
        line.push(0);
        Ok(line)
    }

    fn append_quoted(line: &mut Vec<u16>, argument: &[u16]) {
        let quote = argument.is_empty()
            || argument
                .iter()
                .any(|unit| matches!(*unit, SPACE | TAB | DOUBLE_QUOTE));
        if !quote {
            line.extend_from_slice(argument);
            return;
        }

        line.push(DOUBLE_QUOTE);
        let mut backslashes = 0;
        for &unit in argument {
            if unit == BACKSLASH {
                backslashes += 1;
                continue;
            }
            if unit == DOUBLE_QUOTE {
                line.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2 + 1));
            } else {
                line.extend(std::iter::repeat_n(BACKSLASH, backslashes));
            }
            backslashes = 0;
            line.push(unit);
        }
        line.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
        line.push(DOUBLE_QUOTE);
    }

    fn environment_block(command: &InteractiveCommand) -> io::Result<Vec<u16>> {
        let mut entries: Vec<(OsString, OsString)> = Vec::new();
        for (key, value) in std::env::vars_os() {
            apply_environment_change(&mut entries, key, Some(value));
        }
        for (key, value) in &command.environment {
            validate_environment_key(key)?;
            if let Some(value) = value {
                wide(value, "environment value")?;
            }
            apply_environment_change(&mut entries, key.clone(), value.clone());
        }
        entries.sort_by(|left, right| compare_environment_keys(&left.0, &right.0));

        let mut block = Vec::new();
        for (key, value) in &entries {
            block.extend(wide(key, "environment key")?);
            block.push(EQUALS);
            block.extend(wide(value, "environment value")?);
            block.push(0);
        }
        block.push(0);
        if entries.is_empty() {
            block.push(0);
        }
        Ok(block)
    }

    fn apply_environment_change(
        entries: &mut Vec<(OsString, OsString)>,
        key: OsString,
        value: Option<OsString>,
    ) {
        entries.retain(|(existing, _)| !environment_keys_equal(existing, &key));
        if let Some(value) = value {
            entries.push((key, value));
        }
    }

    fn validate_environment_key(key: &OsStr) -> io::Result<()> {
        let units = wide(key, "environment key")?;
        if units.is_empty() || units.contains(&EQUALS) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "environment keys must be nonempty and may not contain '='",
            ));
        }
        Ok(())
    }

    fn environment_keys_equal(left: &OsStr, right: &OsStr) -> bool {
        left.encode_wide()
            .map(fold_ascii_case)
            .eq(right.encode_wide().map(fold_ascii_case))
    }

    fn compare_environment_keys(left: &OsStr, right: &OsStr) -> Ordering {
        let folded = left
            .encode_wide()
            .map(fold_ascii_case)
            .cmp(right.encode_wide().map(fold_ascii_case));
        folded.then_with(|| left.encode_wide().cmp(right.encode_wide()))
    }

    fn fold_ascii_case(unit: u16) -> u16 {
        match unit {
            value if value >= b'a' as u16 && value <= b'z' as u16 => value - 32,
            value => value,
        }
    }

    fn nul_terminated(value: &OsStr, field: &str) -> io::Result<Vec<u16>> {
        let mut units = wide(value, field)?;
        units.push(0);
        Ok(units)
    }

    fn wide(value: &OsStr, field: &str) -> io::Result<Vec<u16>> {
        let units: Vec<u16> = value.encode_wide().collect();
        if units.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{field} contains a NUL code unit"),
            ));
        }
        Ok(units)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{InteractiveChild, InteractiveCommand};
    use std::{io, process::ExitStatus};

    pub(super) struct Child {
        child: std::process::Child,
    }

    impl Child {
        pub(super) fn id(&self) -> u32 {
            self.child.id()
        }

        pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            self.child.try_wait()
        }

        pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
            self.child.wait()
        }

        pub(super) fn terminate(&mut self) -> io::Result<()> {
            self.child.kill()
        }
    }

    pub(super) fn spawn(
        command: &InteractiveCommand,
        _create_new_console: bool,
    ) -> io::Result<InteractiveChild> {
        let mut process = std::process::Command::new(&command.program);
        process.args(&command.arguments);
        if let Some(directory) = &command.current_dir {
            process.current_dir(directory);
        }
        for (key, value) in &command.environment {
            match value {
                Some(value) => {
                    process.env(key, value);
                }
                None => {
                    process.env_remove(key);
                }
            }
        }
        process.spawn().map(|child| InteractiveChild {
            inner: Child { child },
        })
    }
}

/// Observes one existing process by id without owning it.
///
/// On Windows the handle is opened once, so a later process that reuses the
/// id is never mistaken for the watched one. The watch never terminates.
pub struct ProcessWatch {
    inner: watch::Watch,
}

impl ProcessWatch {
    pub fn open(process_id: u32) -> io::Result<Self> {
        watch::Watch::open(process_id).map(|inner| Self { inner })
    }

    pub fn process_id(&self) -> u32 {
        self.inner.process_id()
    }

    /// `true` once the watched process has exited.
    pub fn has_exited(&self) -> io::Result<bool> {
        self.inner.has_exited()
    }

    /// The executable image of the watched process, read through the held
    /// handle, so a reused process id can never answer for it.
    pub fn image_path(&self) -> io::Result<std::path::PathBuf> {
        self.inner.image_path()
    }
}

#[cfg(windows)]
mod watch {
    use std::{
        io,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            QueryFullProcessImageNameW, WaitForSingleObject,
        },
    };

    pub(super) struct Watch {
        process_id: u32,
        handle: OwnedHandle,
    }

    impl Watch {
        pub(super) fn open(process_id: u32) -> io::Result<Self> {
            // SAFETY: OpenProcess has no pointer arguments; a null answer is an error.
            let raw = unsafe {
                OpenProcess(
                    PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    process_id,
                )
            };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: raw is a fresh process handle this value now owns.
            let handle = unsafe { OwnedHandle::from_raw_handle(raw as _) };
            Ok(Self { process_id, handle })
        }

        pub(super) fn process_id(&self) -> u32 {
            self.process_id
        }

        pub(super) fn has_exited(&self) -> io::Result<bool> {
            // SAFETY: the owned handle stays open for the duration of the call.
            match unsafe { WaitForSingleObject(self.handle.as_raw_handle() as _, 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                WAIT_FAILED => Err(io::Error::last_os_error()),
                other => Err(io::Error::other(format!(
                    "unexpected process wait result {other}"
                ))),
            }
        }

        pub(super) fn image_path(&self) -> io::Result<std::path::PathBuf> {
            use std::os::windows::ffi::OsStringExt;
            let mut buffer = vec![0u16; 32_768];
            let mut size = buffer.len() as u32;
            // SAFETY: the buffer outlives the call and size names its capacity;
            // flag 0 is PROCESS_NAME_WIN32.
            let ok = unsafe {
                QueryFullProcessImageNameW(
                    self.handle.as_raw_handle() as _,
                    0,
                    buffer.as_mut_ptr(),
                    &mut size,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            buffer.truncate(size as usize);
            Ok(std::ffi::OsString::from_wide(&buffer).into())
        }
    }
}

#[cfg(not(windows))]
mod watch {
    use std::{io, path::PathBuf};

    pub(super) struct Watch {
        process_id: u32,
        proc_dir: PathBuf,
    }

    impl Watch {
        pub(super) fn open(process_id: u32) -> io::Result<Self> {
            let proc_dir = PathBuf::from(format!("/proc/{process_id}"));
            if !std::path::Path::new("/proc/self").exists() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "process watch needs /proc on this platform",
                ));
            }
            if !proc_dir.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "process is not running",
                ));
            }
            Ok(Self {
                process_id,
                proc_dir,
            })
        }

        pub(super) fn process_id(&self) -> u32 {
            self.process_id
        }

        pub(super) fn has_exited(&self) -> io::Result<bool> {
            Ok(!self.proc_dir.exists())
        }

        pub(super) fn image_path(&self) -> io::Result<PathBuf> {
            std::fs::read_link(self.proc_dir.join("exe"))
        }
    }
}
