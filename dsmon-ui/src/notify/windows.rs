//! Notifications as a tray balloon, the way the previous build raised them.
//! Windows turns those into toasts of its own on Windows 10 and later.
//!
//! The balloon has to name the tray icon it belongs to. `tray-icon` keeps its
//! own identity private, so the icon is registered under a fixed GUID — the one
//! below — and the same GUID is used here to address it.

use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_GUID, NIF_INFO, NIIF_INFO, NIIF_WARNING, NIM_MODIFY, NOTIFYICONDATAW,
};

use super::Message;

/// The identity the tray icon is registered under. Constant so that the balloon
/// and the icon agree across runs.
pub const ICON_GUID: u128 = 0x8f0c_1a5e_4b6d_49a3_9c21_7e5d_3f80_1b64;

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
        uFlags: NIF_INFO | NIF_GUID,
        guidItem: GUID::from_u128(ICON_GUID),
        dwInfoFlags: if message.title.contains('\u{26a0}') {
            NIIF_WARNING
        } else {
            NIIF_INFO
        },
        ..unsafe { std::mem::zeroed() }
    };

    write_wide(&mut info.szInfoTitle, &message.title);
    write_wide(&mut info.szInfo, &message.body);

    let shown = unsafe { Shell_NotifyIconW(NIM_MODIFY, &info) };
    if shown == 0 {
        return Err("the shell refused the balloon".to_owned());
    }
    Ok(())
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
