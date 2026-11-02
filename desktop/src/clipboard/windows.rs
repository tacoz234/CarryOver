use super::{Clipboard, Content, dib};
use anyhow::{Context, Result, bail};
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows::core::{PCWSTR, w};

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

fn format(name: PCWSTR) -> u32 {
    unsafe { RegisterClipboardFormatW(name) }
}

fn available(format: u32) -> bool {
    format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// Formats that mark content as not for syncing. Password managers set these.
fn marked_private() -> bool {
    available(format(w!("ExcludeClipboardContentFromMonitorProcessing")))
        || available(format(w!("Clipboard Viewer Ignore")))
}

/// Copies a format's bytes. Must be called with the clipboard open.
fn bytes(format: u32) -> Result<Vec<u8>> {
    let data = unsafe { GetClipboardData(format) }.context("reading the clipboard")?;
    let global = HGLOBAL(data.0);
    unsafe {
        let locked = GlobalLock(global) as *const u8;
        if locked.is_null() {
            bail!("could not lock clipboard memory");
        }
        let copy = std::slice::from_raw_parts(locked, GlobalSize(global)).to_vec();
        let _ = GlobalUnlock(global);
        Ok(copy)
    }
}

/// Puts bytes on the clipboard. Must be called with the clipboard open and emptied.
fn put(format: u32, data: &[u8]) -> Result<()> {
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, data.len()).context("allocating")?;
        let locked = GlobalLock(global) as *mut u8;
        if locked.is_null() {
            let _ = GlobalFree(Some(global));
            bail!("could not lock clipboard memory");
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), locked, data.len());
        let _ = GlobalUnlock(global);
        // The system owns the memory once SetClipboardData succeeds.
        if let Err(error) = SetClipboardData(format, Some(HANDLE(global.0))) {
            let _ = GlobalFree(Some(global));
            return Err(error).context("writing the clipboard");
        }
    }
    Ok(())
}

impl Clipboard for WindowsClipboard {
    fn change_token(&self) -> u64 {
        unsafe { GetClipboardSequenceNumber() as u64 }
    }

    fn read(&self) -> Result<Option<Content>> {
        if marked_private() {
            return Ok(None);
        }
        let text = CF_UNICODETEXT.0 as u32;
        let png = format(w!("PNG"));
        let dib = CF_DIB.0 as u32;
        if available(text) {
            let _open = OpenGuard::acquire()?;
            let raw = bytes(text)?;
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_le_bytes(pair))
                .collect();
            let len = units.iter().position(|&u| u == 0).unwrap_or(units.len());
            return Ok(Some(Content::Text(String::from_utf16_lossy(&units[..len]))));
        }
        if available(png) {
            let _open = OpenGuard::acquire()?;
            return Ok(Some(Content::Image(dib::trim_png(&bytes(png)?).to_vec())));
        }
        if available(dib) {
            let raw = {
                let _open = OpenGuard::acquire()?;
                bytes(dib)?
            };
            return Ok(Some(Content::Image(dib::dib_to_png(&raw)?)));
        }
        Ok(None)
    }

    fn write(&self, content: &Content) -> Result<()> {
        // Converted before opening, so the clipboard isn't held during the work.
        let bitmap = match content {
            Content::Image(png) => Some(dib::png_to_dib(png)?),
            Content::Text(_) => None,
        };
        let _open = OpenGuard::acquire()?;
        unsafe { EmptyClipboard() }.context("emptying the clipboard")?;
        match content {
            Content::Text(text) => {
                let units: Vec<u8> = text
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                put(CF_UNICODETEXT.0 as u32, &units)
            }
            Content::Image(png) => {
                // Modern apps read PNG, which keeps transparency. Older ones read the bitmap.
                put(format(w!("PNG")), png)?;
                put(CF_DIB.0 as u32, &bitmap.unwrap())
            }
        }
    }
}
