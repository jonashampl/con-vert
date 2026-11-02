//! Small images for the ring, as PNG data URLs:
//! - the patch of screen behind the ring (the UI blurs it for the frosted-glass look,
//!   which works on Windows 10 and 11 and on a round shape, unlike DWM acrylic),
//! - the Explorer thumbnail of the dragged file for the centre hub.

use base64::Engine;
use windows::core::*;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, IBindCtx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK};

/// Backdrop is blurred anyway, so a small capture is plenty and keeps the event tiny.
const BACKDROP_PX: i32 = 96;

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.write_header().ok()?.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}

/// Reads a bitmap (not selected into any DC) as top-down RGBA.
unsafe fn bitmap_rgba(hbm: HBITMAP, w: i32, h: i32, keep_alpha: bool) -> Option<Vec<u8>> {
    unsafe {
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let screen = GetDC(None);
        let lines = GetDIBits(screen, hbm, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, screen);
        if lines == 0 {
            return None;
        }
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // BGRA -> RGBA
            if !keep_alpha {
                px[3] = 255;
            } else if px[3] > 0 && px[3] < 255 {
                // shell thumbnails are premultiplied
                let a = px[3] as u32;
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Some(buf)
    }
}

/// The screen rect at (x, y) of `w`x`h` physical pixels, scaled to `out_w`x`out_h`, as RGBA.
/// Everything on screen is in it, the floating icon included.
pub fn screen_rgba(x: i32, y: i32, w: i32, h: i32, out_w: i32, out_h: i32) -> Option<Vec<u8>> {
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, out_w, out_h);
        let old = SelectObject(mem, bmp);
        SetStretchBltMode(mem, HALFTONE);
        let ok = StretchBlt(mem, 0, 0, out_w, out_h, screen, x, y, w, h, SRCCOPY).as_bool();
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let rgba = if ok { bitmap_rgba(bmp, out_w, out_h, false) } else { None };
        let _ = DeleteObject(bmp);
        rgba
    }
}

/// The desktop itself (wallpaper and desktop icons) as drawn by the shell, without any
/// window on top: what the floating icon sits on. Rendering it takes tens of ms, so it's
/// cached and redrawn at most every DESKTOP_MAX_AGE, or when `fresh` asks for it.
struct DesktopCache {
    at: std::time::Instant,
    /// Desktop window rect in screen pixels; `rgba` is its top-down image.
    rect: windows::Win32::Foundation::RECT,
    rgba: Vec<u8>,
}

static DESKTOP: std::sync::Mutex<Option<DesktopCache>> = std::sync::Mutex::new(None);
const DESKTOP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(5);

unsafe fn render_desktop() -> Option<DesktopCache> {
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowRect};
    unsafe {
        let shell = GetShellWindow();
        if shell.is_invalid() {
            return None;
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        GetWindowRect(shell, &mut rect).ok()?;
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        // PW_RENDERFULLCONTENT (2): the composed desktop, wallpaper included.
        let ok = PrintWindow(shell, mem, PRINT_WINDOW_FLAGS(2)).as_bool();
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        let rgba = if ok { bitmap_rgba(bmp, w, h, false) } else { None };
        let _ = DeleteObject(bmp);
        Some(DesktopCache { at: std::time::Instant::now(), rect, rgba: rgba? })
    }
}

static RENDERING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn refresh_desktop() {
    if let Some(fresh) = unsafe { render_desktop() } {
        *DESKTOP.lock().unwrap_or_else(|e| e.into_inner()) = Some(fresh);
    }
}

/// The desktop under the screen rect at (x, y) of `w`x`h` physical pixels, as RGBA. The
/// floating icon's glass uses this instead of a screen capture, so the icon shows up in
/// screenshots and recordings without its glass feeding on its own pixels.
pub fn desktop_rgba(x: i32, y: i32, w: i32, h: i32, fresh: bool) -> Option<Vec<u8>> {
    let stale = DESKTOP.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|c| fresh || c.at.elapsed() > DESKTOP_MAX_AGE);
    match stale {
        // First time: draw it now (at startup, nothing is waiting on this thread yet).
        None => refresh_desktop(),
        // Later on a thread of its own: PrintWindow waits on Explorer, and the thread
        // calling this also runs the drop target, which must never stall.
        Some(true) if !RENDERING.swap(true, std::sync::atomic::Ordering::SeqCst) => {
            std::thread::spawn(|| {
                refresh_desktop();
                RENDERING.store(false, std::sync::atomic::Ordering::SeqCst);
            });
        }
        _ => {}
    }
    let cache = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
    let c = cache.as_ref()?;
    let (cw, ch) = (c.rect.right - c.rect.left, c.rect.bottom - c.rect.top);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for row in 0..h {
        let sy = (y - c.rect.top + row).clamp(0, ch - 1);
        for col in 0..w {
            let sx = (x - c.rect.left + col).clamp(0, cw - 1);
            let (s, d) = (((sy * cw + sx) * 4) as usize, ((row * w + col) * 4) as usize);
            out[d..d + 4].copy_from_slice(&c.rgba[s..s + 4]);
        }
    }
    Some(out)
}

/// Captures the screen rect at (x, y) of `w`x`h` physical pixels, downscaled; the UI
/// stretches it back over the same area and blurs it.
pub fn backdrop(x: i32, y: i32, w: i32, h: i32) -> Option<String> {
    encode_png(BACKDROP_PX as u32, BACKDROP_PX as u32, &screen_rgba(x, y, w, h, BACKDROP_PX, BACKDROP_PX)?)
}

/// The screen behind `hwnd`'s own rect with that window left out of the capture for a
/// moment, so a visible card can renew its glass. Screenshots of it still work afterwards.
pub fn backdrop_behind(hwnd: isize, x: i32, y: i32, w: i32, h: i32) -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};
    let win = HWND(hwnd as *mut _);
    unsafe { SetWindowDisplayAffinity(win, WDA_EXCLUDEFROMCAPTURE).ok()? };
    // The next composed frame drops it; capture after that.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let shot = backdrop(x, y, w, h);
    unsafe {
        let _ = SetWindowDisplayAffinity(win, WDA_NONE);
    }
    shot
}

/// Explorer's thumbnail (or icon) for `path`. Call from a worker thread.
pub fn thumbnail(path: &str, size: i32) -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None::<&IBindCtx>).ok()?;
        let hbm = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_BIGGERSIZEOK).ok()?;
        let mut info = BITMAP::default();
        GetObjectW(hbm, std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut _ as *mut _));
        let rgba = bitmap_rgba(hbm, info.bmWidth, info.bmHeight, true);
        let _ = DeleteObject(hbm);
        encode_png(info.bmWidth as u32, info.bmHeight as u32, &rgba?)
    }
}
