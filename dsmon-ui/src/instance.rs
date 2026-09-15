//! One instance at a time.
//!
//! Starting a program again is not starting a second one: the copy that is
//! already running raises its window and the new process leaves, which keeps a
//! second tray icon, a second poller and a second widget out of the picture.
//!
//! Linux claims a name on the session bus, which a second process can also talk
//! to. Windows claims a named mutex, and a named event carries the request
//! across. Where neither is available the program simply runs: being unable to
//! check is not a reason to refuse to start.

use std::sync::{Arc, Mutex, OnceLock};

/// The names a program claims, one set each for the application and the widget.
///
/// They have to be apart: sharing them would make a widget started while only
/// the application runs hand its request to the application and leave, and the
/// only thing on screen would be the window that was already there.
#[derive(Debug, Clone, Copy)]
pub struct Names {
    /// The name a copy claims on the session bus, which is how Linux does it.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub service: &'static str,
    /// Windows holds a mutex, and a second launch sets an event.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub mutex: &'static str,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub event: &'static str,
}

impl Names {
    /// The application: the window, the tray and the poller.
    pub const APPLICATION: Self = Self {
        service: "com.github.wenyinos.deepseek-balance-monitor",
        mutex: "Local\\DeepSeekBalanceMonitor",
        event: "Local\\DeepSeekBalanceMonitorShow",
    };

    /// The desktop widget, which has a window of its own to raise.
    pub const WIDGET: Self = Self {
        service: "com.github.wenyinos.deepseek-balance-monitor-widget",
        mutex: "Local\\DeepSeekBalanceMonitorWidget",
        event: "Local\\DeepSeekBalanceMonitorWidgetShow",
    };
}

/// What a second launch asks of the copy that is already running.
///
/// The only thing a second launch can want is the window it did not get, so
/// there is one request and the program decides what raising it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Bring the window to the front.
    Show,
}

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use zbus::fdo::{RequestNameFlags, RequestNameReply};

    use super::{Guard, Names, Request};

    /// Where the request is answered. The name is only unique within one
    /// process — a caller names the service it talks to — so the application
    /// and the widget share this one, and their two names keep them apart.
    const PATH: &str = "/app";
    const INTERFACE: &str = "com.github.wenyinos.DeepseekBalanceMonitor";

    /// The claim, held for as long as the process runs.
    pub struct Claim {
        _connection: zbus::blocking::Connection,
    }

    /// The one request another launch can make.
    pub struct ShowRequest {
        requests: Arc<Mutex<Vec<Request>>>,
        ctx: Arc<OnceLock<egui::Context>>,
    }

    #[zbus::interface(name = "com.github.wenyinos.DeepseekBalanceMonitor")]
    impl ShowRequest {
        /// Raise the window of the copy that is already running.
        fn show(&self) {
            if let Ok(mut queue) = self.requests.lock() {
                queue.push(Request::Show);
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
        names: Names,
        requests: Arc<Mutex<Vec<Request>>>,
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
            .at(PATH, ShowRequest { requests, ctx })
        {
            note(&format!("the request handler could not be served: {error}"));
            return Ok(None);
        }

        // Asking for the name is the whole check: the answer says whether
        // anyone else is the application already.
        let flags = RequestNameFlags::DoNotQueue.into();
        match connection.request_name_with_flags(names.service, flags) {
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
    pub fn ask_to_show(names: Names) -> Result<(), String> {
        let connection =
            zbus::blocking::Connection::session().map_err(|error| error.to_string())?;

        // The copy that is starting may not have installed its interface yet,
        // which is a matter of a few milliseconds.
        let mut last = String::new();
        for attempt in 0..5 {
            match connection.call_method(Some(names.service), PATH, Some(INTERFACE), "Show", &()) {
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

    use super::{Guard, Names, Request};

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
        names: Names,
        requests: Arc<Mutex<Vec<Request>>>,
        ctx: Arc<OnceLock<egui::Context>>,
    ) -> Result<Option<Guard>, ()> {
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide(names.event).as_ptr()) };
        if event.is_null() {
            // Cannot tell who is first, so run.
            return Ok(None);
        }

        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, wide(names.mutex).as_ptr()) };
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
                if let Ok(mut queue) = requests.lock() {
                    queue.push(Request::Show);
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
    pub fn ask_to_show(_names: Names) -> Result<(), String> {
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
    names: Names,
    quiet: bool,
    requests: Arc<Mutex<Vec<Request>>>,
    ctx: Arc<OnceLock<egui::Context>>,
) -> Role {
    match platform::claim(names, requests, ctx) {
        Ok(guard) => Role::First(guard.unwrap_or(Guard { claim: None })),
        Err(()) => {
            if !quiet {
                match platform::ask_to_show(names) {
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
