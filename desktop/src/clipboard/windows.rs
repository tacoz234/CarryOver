use super::Clipboard;
use anyhow::{Context, Result, bail};
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::core::w;

pub struct WindowsClipboard;

/// Holds the clipboard open. Other apps hold it briefly too, hence the retries.
struct OpenGuard;

impl OpenGuard {
    fn acquire() -> Result<Self> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            sleep(Duration::from_millis(20));
        }
        bail!("another app is holding the clipboard open")
    }
}

impl Drop for OpenGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// Formats that mark content as not for syncing. Password managers set these.
fn marked_private() -> bool {
    let formats = [
        w!("ExcludeClipboardContentFromMonitorProcessing"),
        w!("Clipboard Viewer Ignore"),
    ];
    formats.iter().any(|name| {
        let format = unsafe { RegisterClipboardFormatW(*name) };
        format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    })
}

impl Clipboard for WindowsClipboard {
    fn change_token(&self) -> u64 {
        unsafe { GetClipboardSequenceNumber() as u64 }
    }

    fn read_text(&self) -> Result<Option<String>> {
        let format = CF_UNICODETEXT.0 as u32;
        if marked_private() || unsafe { IsClipboardFormatAvailable(format) }.is_err() {
            return Ok(None);
        }
        let _open = OpenGuard::acquire()?;
        let data = unsafe { GetClipboardData(format) }.context("reading clipboard text")?;
        let global = HGLOBAL(data.0);
        unsafe {
            let locked = GlobalLock(global) as *const u16;
            if locked.is_null() {
                bail!("could not lock clipboard memory");
            }
            let capacity = GlobalSize(global) / 2;
            let units = std::slice::from_raw_parts(locked, capacity);
            let len = units.iter().position(|&u| u == 0).unwrap_or(capacity);
            let text = String::from_utf16_lossy(&units[..len]);
            let _ = GlobalUnlock(global);
            Ok(Some(text))
        }
    }

    fn write_text(&self, text: &str) -> Result<()> {
        let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let _open = OpenGuard::acquire()?;
        unsafe {
            EmptyClipboard().context("emptying clipboard")?;
            let global = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2).context("allocating")?;
            let locked = GlobalLock(global) as *mut u16;
            if locked.is_null() {
                let _ = GlobalFree(Some(global));
                bail!("could not lock clipboard memory");
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), locked, units.len());
            let _ = GlobalUnlock(global);
            // The system owns the memory once SetClipboardData succeeds.
            if let Err(error) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(global.0))) {
                let _ = GlobalFree(Some(global));
                return Err(error).context("writing clipboard text");
            }
        }
        Ok(())
    }
}
