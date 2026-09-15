//! The widget's link to the application.
//!
//! The widget reads; it never talks to a provider, opens the database or looks
//! at a key. All it does is ask the local interface for a payload, and when
//! there is no answer it says so instead of showing something made up. The
//! asking happens on this thread so a slow or absent answer cannot stall a
//! frame.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use dsmon_core::widget_api::{self, Payload, PORT};

/// How often the interface is asked while it answers.
const POLL: Duration = Duration::from_secs(2);

/// How often it is tried again while it does not. Slower on purpose: an absent
/// application is the normal case when the widget is started by hand, and
/// hammering a closed port every two seconds buys nothing.
const RETRY: Duration = Duration::from_secs(10);

/// What the widget knows at this moment.
#[derive(Debug, Clone, Default)]
pub struct LinkState {
    /// The last payload that arrived. Kept when the application goes away: it
    /// is still the last thing that was true, and the panel greys it out rather
    /// than throwing away the numbers.
    pub payload: Option<Payload>,
    /// Whether the last attempt was answered.
    pub connected: bool,
    /// When the last attempt was made.
    pub checked: Option<Instant>,
    /// The application answered with a payload version this widget does not
    /// know. The contract asks a client to say so rather than showing whichever
    /// half happens to parse.
    pub mismatched: bool,
}

/// A running link. Dropping it ends the thread.
pub struct Link {
    state: Arc<Mutex<LinkState>>,
    range: Arc<Mutex<u64>>,
    /// Set when the next attempt should be `/check`, which asks the application
    /// for a poll instead of handing back what it already has.
    urgent: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Link {
    /// Starts asking, beginning with `range` days of history.
    pub fn start(range: u64) -> Self {
        let state = Arc::new(Mutex::new(LinkState::default()));
        let days = Arc::new(Mutex::new(range));
        let urgent = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_state = Arc::clone(&state);
        let thread_range = Arc::clone(&days);
        let thread_urgent = Arc::clone(&urgent);
        let thread_stop = Arc::clone(&stop);

        let thread = thread::Builder::new()
            .name("dsmon-widget-link".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    let path = if thread_urgent.swap(false, Ordering::Relaxed) {
                        "/check".to_owned()
                    } else {
                        let days = thread_range.lock().map(|days| *days).unwrap_or(7);
                        format!("/widget-status?days={days}")
                    };

                    let answer = widget_api::fetch(PORT, &path);
                    let connected = answer.is_ok();
                    if let Ok(mut state) = thread_state.lock() {
                        state.connected = connected;
                        state.checked = Some(Instant::now());
                        if let Ok(payload) = answer {
                            state.mismatched = payload.version != widget_api::VERSION;
                            state.payload = Some(payload);
                        }
                    }

                    let wait = if connected { POLL } else { RETRY };
                    let until = Instant::now() + wait;
                    while Instant::now() < until && !thread_stop.load(Ordering::Relaxed) {
                        // Stepped, so a stop or a button press is noticed
                        // without waiting out the whole interval.
                        thread::sleep(Duration::from_millis(100));
                        if thread_urgent.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            })
            .ok();

        Self {
            state,
            range: days,
            urgent,
            stop,
            thread,
        }
    }

    /// The current picture.
    pub fn state(&self) -> LinkState {
        self.state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    /// Days of history the curve should cover. Asking for a different range
    /// fetches at once rather than at the end of the current wait.
    pub fn range(&self) -> u64 {
        self.range.lock().map(|days| *days).unwrap_or(7)
    }

    pub fn set_range(&self, days: u64) {
        if let Ok(mut range) = self.range.lock() {
            *range = days;
        }
        self.urgent.store(true, Ordering::Relaxed);
    }

    /// Asks the application for a poll, then for the reading it produces.
    pub fn check_now(&self) {
        self.urgent.store(true, Ordering::Relaxed);
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts the application beside the widget.
///
/// The application takes care of the rest itself: it is single-instance, so
/// pressing the button while it already runs only raises the window that is
/// there.
pub fn start_application() -> Result<(), String> {
    spawn_sibling(if cfg!(windows) {
        "dsmon2.exe"
    } else {
        "dsmon2"
    })
}

/// Starts a program that sits in this one's own directory.
///
/// Both the widget and the application are built and installed side by side,
/// so whichever one is running knows where the other is.
pub(crate) fn spawn_sibling(name: &str) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .map(|directory| directory.join(name))
        .ok_or_else(|| "the program's own directory could not be read".to_owned())?;

    std::process::Command::new(&executable)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", executable.display()))
}
