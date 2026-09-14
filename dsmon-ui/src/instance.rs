//! One instance at a time.
//!
//! Starting the application again is not starting a second one: the copy that
//! is already running raises its window and the new process leaves, which keeps
//! a second tray icon and a second poller out of the picture.
//!
//! Linux claims a name on the session bus, which a second process can also talk
//! to. Windows claims a named mutex, and a named event carries the request
//! across. Where neither is available the application simply runs: being unable
//! to check is not a reason to refuse to start.

use std::sync::{Arc, Mutex, OnceLock};

use crate::tray::Command;

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use zbus::fdo::{RequestNameFlags, RequestNameReply};

    use crate::tray::Command;

    use super::Guard;

    /// The name the running copy answers to, and where it answers.
    const SERVICE: &str = "com.github.wenyinos.deepseek-balance-monitor";
    const PATH: &str = "/app";
    const INTERFACE: &str = "com.github.wenyinos.DeepseekBalanceMonitor";

    /// The claim, held for as long as the process runs.
    pub struct Claim {
        _connection: zbus::blocking::Connection,
    }

    /// The one request another launch can make.
    pub struct ShowRequest {
        commands: Arc<Mutex<Vec<Command>>>,
        ctx: Arc<OnceLock<egui::Context>>,
    }

    #[zbus::interface(name = "com.github.wenyinos.DeepseekBalanceMonitor")]
    impl ShowRequest {
        /// Raise the window of the copy that is already running.
        fn show(&self) {
            if let Ok(mut queue) = self.commands.lock() {
                queue.push(Command::OpenWindow);
            }
            // Waking the interface is only possible once it exists; before that
            // the command waits in the queue for its first frame.
            if let Some(ctx) = self.ctx.get() {
                ctx.request_repaint();
            }
        }
    }

    /// Takes the name, or reports that someone else holds it.
    pub fn claim(
        commands: Arc<Mutex<Vec<Command>>>,
        ctx: Arc<OnceLock<egui::Context>>,
    ) -> Result<Option<Guard>, ()> {
        let connection = match zbus::blocking::Connection::session() {
            Ok(connection) => connection,
            // Nothing to claim against: run on our own.
            Err(error) => {
                note(&format!("no session bus to claim on: {error}"));
                return Ok(None);
            }
        };

        // The interface has to be in place before the name is taken, or a
        // request arriving in between would have nothing to reach.
        if let Err(error) = connection
            .object_server()
            .at(PATH, ShowRequest { commands, ctx })
        {
            note(&format!("the request handler could not be served: {error}"));
            return Ok(None);
        }

        // Asking for the name is the whole check: the answer says whether
        // anyone else is the application already.
        let flags = RequestNameFlags::DoNotQueue.into();
        match connection.request_name_with_flags(SERVICE, flags) {
            Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => {
                Ok(Some(Guard::from(Claim {
                    _connection: connection,
                })))
            }
            Err(zbus::Error::NameTaken) => Err(()),
            Ok(_) => Err(()),
            Err(error) => {
                note(&format!("the name could not be requested: {error}"));
                Ok(None)
            }
        }
    }

    /// Asks the copy that holds the name to raise its window.
    pub fn ask_to_show() -> Result<(), String> {
        let connection =
            zbus::blocking::Connection::session().map_err(|error| error.to_string())?;

        // The copy that is starting may not have installed its interface yet,
        // which is a matter of a few milliseconds.
        let mut last = String::new();
        for attempt in 0..5 {
            match connection.call_method(Some(SERVICE), PATH, Some(INTERFACE), "Show", &()) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    last = error.to_string();
                    if attempt < 4 {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            }
        }
        Err(last)
    }

    fn note(message: &str) {
        let _ = dsmon_core::storage::log_line(&format!(
            "Running without a single-instance claim: {message}"
        ));
    }
}

#[cfg(windows)]
mod platform {
    use std::sync::{Arc, Mutex, OnceLock};

    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE,
    };

    use crate::tray::Command;

    use super::Guard;

    /// Per session, which for a tray application is the same as per user.
    const MUTEX_NAME: &str = "Local\\DeepSeekBalanceMonitor";
    const EVENT_NAME: &str = "Local\\DeepSeekBalanceMonitorShow";

    /// The claim: the mutex that says who was first, and the event the copy
    /// that was not first sets.
    pub struct Claim {
        _mutex: HANDLE,
        event: HANDLE,
    }

    /// Windows handles are plain pointers; a claim is used from one thread only.
    unsafe impl Send for Claim {}

    /// Takes the mutex, or reports that someone else holds it.
    pub fn claim(
        commands: Arc<Mutex<Vec<Command>>>,
        ctx: Arc<OnceLock<egui::Context>>,
    ) -> Result<Option<Guard>, ()> {
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide(EVENT_NAME).as_ptr()) };
        if event.is_null() {
            // Cannot tell who is first, so run.
            return Ok(None);
        }

        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, wide(MUTEX_NAME).as_ptr()) };
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if mutex.is_null() {
            return Ok(None);
        }

        if already_running {
            // Keep the event handle until it has been set; dropping it closes
            // it, which would make the set meaningless.
            unsafe { SetEvent(event) };
            unsafe { windows_sys::Win32::Foundation::CloseHandle(event) };
            unsafe { windows_sys::Win32::Foundation::CloseHandle(mutex) };
            return Err(());
        }

        // A handle is a pointer, which a thread cannot capture: it travels as
        // the number the operating system gave it.
        let waited_on = event as isize;
        std::thread::spawn(move || {
            let event = waited_on as HANDLE;
            while unsafe { WaitForSingleObject(event, INFINITE) } == 0 {
                if let Ok(mut queue) = commands.lock() {
                    queue.push(Command::OpenWindow);
                }
                if let Some(ctx) = ctx.get() {
                    ctx.request_repaint();
                }
            }
        });

        Ok(Some(Guard::from(Claim {
            _mutex: mutex,
            event,
        })))
    }

    /// Nothing to ask for: the event was set while claiming.
    pub fn ask_to_show() -> Result<(), String> {
        Ok(())
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

/// The claim, kept alive for as long as the process runs.
pub struct Guard {
    #[allow(dead_code)]
    claim: Option<platform::Claim>,
}

impl From<platform::Claim> for Guard {
    fn from(claim: platform::Claim) -> Self {
        Self { claim: Some(claim) }
    }
}

/// What this process turned out to be.
pub enum Role {
    /// Run: this is the copy that holds the window, or the only one that could
    /// be checked for.
    First(Guard),
    /// Another copy is already up and has been asked to show its window.
    Latecomer,
}

/// Claims the right to be the running copy.
///
/// `quiet` marks a start that only wanted the tray — one asked for by the
/// session at login — which has nothing to raise and nothing to say.
pub fn claim(
    quiet: bool,
    commands: Arc<Mutex<Vec<Command>>>,
    ctx: Arc<OnceLock<egui::Context>>,
) -> Role {
    match platform::claim(commands, ctx) {
        Ok(guard) => Role::First(guard.unwrap_or(Guard { claim: None })),
        Err(()) => {
            if !quiet {
                match platform::ask_to_show() {
                    Ok(()) => {
                        let _ =
                            dsmon_core::storage::log_line("Asked the running copy for its window");
                    }
                    Err(error) => {
                        let _ = dsmon_core::storage::log_line(&format!(
                            "Could not reach the running copy: {error}"
                        ));
                    }
                }
            }
            Role::Latecomer
        }
    }
}
