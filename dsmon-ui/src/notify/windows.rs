//! Notifications as a tray balloon, the way the previous build raised them.
//! Windows turns those into toasts of its own on Windows 10 and later.
//!
//! The balloon has to name the tray icon it belongs to: a window and a number.
//! The number is the one `tray-icon` gave the icon when it registered it, and
//! the library keeps it to itself, so what it most likely is, is written down
//! and the shell is asked when that turns out to be wrong.

use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{HWND, RECT, S_OK};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_INFO, NIIF_NONE, NIM_MODIFY, NOTIFYICONDATAW,
    NOTIFYICONIDENTIFIER,
};

use super::Message;

/// The number the tray icon is expected to have.
///
/// The library numbers its icons from one and draws two numbers for each of
/// ours: one for the string id the builder is handed, another for the icon the
/// shell is told about. A process that makes a single icon therefore has it
/// under number two.
const ICON_ID: u32 = 2;

/// How far to look for the icon when the number above is not the one the shell
/// knows: one icon costs two numbers, and an icon registered again would cost
/// two more, so this covers every way one process's icons can be numbered.
const HIGHEST_ICON_ID: u32 = 8;

/// The hidden window `tray-icon` created for our icon, remembered when the tray
/// is set up.
static ICON_WINDOW: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Called once, when the tray icon is created.
pub fn remember_icon(hwnd: HWND) {
    ICON_WINDOW.store(hwnd, Ordering::SeqCst);
}

/// Raises the balloon.
pub fn send(message: &Message) -> Result<(), String> {
    let hwnd = ICON_WINDOW.load(Ordering::SeqCst);
    if hwnd.is_null() {
        return Err("the tray icon is not up yet".to_owned());
    }

    let mut info = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uFlags: NIF_INFO,
        uID: ICON_ID,
        // No mark beside the message: the desktop shows none for its own
        // notifications either, and the shell's warning and information marks
        // are louder than a balance reading.
        dwInfoFlags: NIIF_NONE,
        ..unsafe { std::mem::zeroed() }
    };

    write_wide(&mut info.szInfoTitle, &message.title);
    write_wide(&mut info.szInfo, &message.body);

    let mut shown = unsafe { Shell_NotifyIconW(NIM_MODIFY, &info) };
    if shown == 0 {
        // The shell holds the icon under a number it was not told, so it is
        // asked which one it is: a reading that was asked for is worth the
        // second attempt.
        let Some(number) = icon_number(hwnd) else {
            return Err("the shell does not hold the tray icon".to_owned());
        };
        info.uID = number;
        shown = unsafe { Shell_NotifyIconW(NIM_MODIFY, &info) };
    }

    if shown == 0 {
        return Err(format!(
            "the shell refused the balloon for icon {}",
            info.uID
        ));
    }
    Ok(())
}

/// The number the shell knows the tray icon by, or nothing when it holds none.
///
/// `Shell_NotifyIconGetRect` answers with a rectangle only for an icon the
/// shell really has, which makes it the one way to ask what number that is.
fn icon_number(hwnd: HWND) -> Option<u32> {
    (1..=HIGHEST_ICON_ID).find(|number| {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: hwnd,
            uID: *number,
            ..Default::default()
        };
        let mut rect = RECT::default();
        unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) == S_OK }
    })
}

/// Copies `text` into a fixed-size wide buffer, cutting it to fit when the
/// shell's limit is smaller than the message.
fn write_wide(buffer: &mut [u16], text: &str) {
    let encoded: Vec<u16> = text.encode_utf16().collect();
    let room = buffer.len().saturating_sub(1);
    let take = encoded.len().min(room);
    buffer[..take].copy_from_slice(&encoded[..take]);
    buffer[take] = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_message_is_cut_to_the_buffer() {
        let mut buffer = [1u16; 8];
        write_wide(&mut buffer, "abcdefghij");

        assert_eq!(&buffer[..7], &[97, 98, 99, 100, 101, 102, 103]);
        assert_eq!(buffer[7], 0, "the text stays terminated");
    }

    #[test]
    fn a_short_message_keeps_its_terminator() {
        let mut buffer = [1u16; 8];
        write_wide(&mut buffer, "hi");

        assert_eq!(&buffer[..3], &[104, 105, 0]);
    }
}
