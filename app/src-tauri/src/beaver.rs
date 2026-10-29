//! The floating icon: a small widget you can drag around and drop files onto. Dropping
//! on it opens the ring centred on the icon, no Shift needed. It's a per-pixel-alpha
//! window, so only the icon's own pixels take clicks and drops; around it, everything
//! passes through to the windows underneath. The icon is liquid glass, redrawn from the
//! screen behind it whenever that changes (core::icon draws it).
//!
//! It sits on the desktop like a gadget, behind every other window, and rises on top only
//! while a file is being dragged (the trigger thread calls [`raise`]), so it never covers
//! your work but is there to drop onto.
//!
//! Lives on the trigger thread (which has OLE set up and a message loop). The app talks
//! to it with [`set_visible`]; it reports back through `Hooks::on_beaver`.

use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};

use image::{Rgba, RgbaImage};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Gap from the screen corner by default, in logical px.
const MARGIN_LOGICAL: f64 = 28.0;
/// Sizes offered in the right-click menu: Windows' small, medium and large desktop icons.
const SIZES: [(usize, PCWSTR, i32); 3] = [(10, w!("Small"), 32), (11, w!("Medium"), 48), (12, w!("Large"), 96)];
/// How often the glass checks whether the screen behind it changed (a window moved away).
const REFRESH_MS: u32 = 1500;
const TIMER_REFRESH: usize = 1;
/// The icon is drawn a little smaller at rest and full size while a file hovers over it.
const REST: f32 = 0.88;
const PERKED: f32 = 1.0;

const WM_APP_VISIBLE: u32 = WM_APP + 20;
const WM_APP_RAISE: u32 = WM_APP + 21;
const ID_SETTINGS: usize = 1;
const ID_HIDE: usize = 2;

/// What the beaver tells the app.
pub enum BeaverEvent {
    /// Dragged to a new place (top-left, physical px).
    Moved(i32, i32),
    /// A new size picked from its right-click menu (logical px).
    Resized(i32),
    /// Hidden from its right-click menu.
    Hidden,
    OpenSettings,
}

static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
static WANT_VISIBLE: AtomicBool = AtomicBool::new(true);
static RAISED: AtomicBool = AtomicBool::new(false);
static SIZE_PX: AtomicI32 = AtomicI32::new(48);
/// Chosen edge in logical px.
static LOGICAL: AtomicI32 = AtomicI32::new(48);
/// Scale last drawn at (f32 bits), and a hash of what was drawn, to skip identical redraws.
static SCALE: AtomicU32 = AtomicU32::new(0);
static DRAWN: AtomicU64 = AtomicU64::new(0);

fn hwnd() -> HWND {
    HWND(HWND_RAW.load(SeqCst) as *mut _)
}

fn emit(e: BeaverEvent) {
    (crate::trigger::hooks().on_beaver)(e)
}

/// Shows or hides the beaver. Safe from any thread, before or after it exists.
pub fn set_visible(on: bool) {
    WANT_VISIBLE.store(on, SeqCst);
    if HWND_RAW.load(SeqCst) != 0 {
        unsafe {
            let _ = PostMessageW(hwnd(), WM_APP_VISIBLE, WPARAM(on as usize), LPARAM(0));
        }
    }
}

/// Brings the beaver above every window (a file drag is under way) or sends it back down
/// to the desktop. Safe from any thread.
pub(crate) fn raise(on: bool) {
    if RAISED.swap(on, SeqCst) != on && HWND_RAW.load(SeqCst) != 0 {
        unsafe {
            let _ = PostMessageW(hwnd(), WM_APP_RAISE, WPARAM(on as usize), LPARAM(0));
        }
    }
}

/// Down to desktop level: owned by the desktop window (Windows keeps an owned window just
/// above its owner, and Show desktop leaves it alone), below every normal window.
unsafe fn sink(h: HWND) {
    unsafe {
        let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        if let Ok(desktop) = FindWindowW(w!("Progman"), None) {
            SetWindowLongPtrW(h, GWLP_HWNDPARENT, desktop.0 as isize);
        }
        let _ = SetWindowPos(h, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// The beaver's screen rect (left, top, right, bottom) while it's showing, so cards can
/// keep clear of it. Safe from any thread.
pub(crate) fn visible_rect() -> Option<(i32, i32, i32, i32)> {
    if HWND_RAW.load(SeqCst) == 0 || !unsafe { IsWindowVisible(hwnd()) }.as_bool() {
        return None;
    }
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd(), &mut r) }.ok()?;
    Some((r.left, r.top, r.right, r.bottom))
}

/// Widget size in physical px for the monitor containing `pt`.
fn size_at(pt: POINT) -> i32 {
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        (LOGICAL.load(SeqCst) as f64 * dx as f64 / 96.0).round() as i32
    }
}

/// Draws the icon at `scale` of the window into the layered window, as glass over the
/// desktop behind it. Skips the redraw when nothing changed.
unsafe fn paint(scale: f32) {
    unsafe {
        let size = SIZE_PX.load(SeqCst);
        let inner = ((size as f32 * scale).round() as u32).max(1);
        let off = (size as u32 - inner) / 2;
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd(), &mut r);
        let (n, x, y) = (inner as i32, r.left + off as i32, r.top + off as i32);
        let behind = crate::capture::desktop_rgba(x, y, n, n, false)
            .and_then(|px| RgbaImage::from_raw(inner, inner, px))
            .unwrap_or_else(|| RgbaImage::from_pixel(inner, inner, Rgba([40, 40, 44, 255])));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (behind.as_raw(), size, scale.to_bits()).hash(&mut h);
        SCALE.store(scale.to_bits(), SeqCst);
        if DRAWN.swap(h.finish(), SeqCst) == h.finish() {
            return;
        }
        let small = shiftdrop_core::icon::glass(&behind);
        // Premultiplied BGRA, top-down, as UpdateLayeredWindow expects.
        let mut px = vec![0u8; (size * size * 4) as usize];
        for (x, y, p) in small.enumerate_pixels() {
            let a = p[3] as u32;
            let i = (((y + off) * size as u32 + x + off) * 4) as usize;
            px[i] = (p[2] as u32 * a / 255) as u8;
            px[i + 1] = (p[1] as u32 * a / 255) as u8;
            px[i + 2] = (p[0] as u32 * a / 255) as u8;
            px[i + 3] = a as u8;
        }
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = size;
        bi.bmiHeader.biHeight = -size;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(mem, &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u8, px.len());
            let old = SelectObject(mem, dib);
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let sz = SIZE { cx: size, cy: size };
            let src = POINT { x: 0, y: 0 };
            let _ = UpdateLayeredWindow(hwnd(), screen, None, Some(&sz), mem, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
            SelectObject(mem, old);
            let _ = DeleteObject(dib);
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
}

/// Redraws at the current scale (the screen behind may have changed).
unsafe fn repaint() {
    // Nothing drawn yet: WM_MOVE also arrives while the window is being created.
    let scale = f32::from_bits(SCALE.load(SeqCst));
    if scale > 0.0 && HWND_RAW.load(SeqCst) != 0 {
        unsafe { paint(scale) }
    }
}

/// Re-fits the widget to the DPI of the monitor it's now on and the chosen size, keeping
/// its centre and keeping it on that monitor's work area.
unsafe fn refit() {
    unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd(), &mut r);
        let centre = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
        let size = size_at(centre);
        if size != SIZE_PX.swap(size, SeqCst) {
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(MonitorFromPoint(centre, MONITOR_DEFAULTTONEAREST), &mut mi);
            let w = mi.rcWork;
            let x = (centre.x - size / 2).clamp(w.left, (w.right - size).max(w.left));
            let y = (centre.y - size / 2).clamp(w.top, (w.bottom - size).max(w.top));
            let _ = SetWindowPos(hwnd(), None, x, y, size, size, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        paint(REST);
    }
}

unsafe fn context_menu(x: i32, y: i32) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let _ = AppendMenuW(menu, MF_STRING, ID_SETTINGS, w!("Settings…"));
        if let Ok(sizes) = CreatePopupMenu() {
            for (id, label, _) in SIZES {
                let _ = AppendMenuW(sizes, MF_STRING, id, label);
            }
            if let Some((id, _, _)) = SIZES.iter().find(|s| s.2 == LOGICAL.load(SeqCst)) {
                let _ = CheckMenuRadioItem(sizes, SIZES[0].0 as u32, SIZES[2].0 as u32, *id as u32, MF_BYCOMMAND.0);
            }
            let _ = AppendMenuW(menu, MF_POPUP, sizes.0 as usize, w!("Size"));
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_HIDE, w!("Hide icon"));
        // Required so the menu closes when you click elsewhere.
        let _ = SetForegroundWindow(hwnd());
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, 0, hwnd(), None).0 as usize;
        let _ = DestroyMenu(menu);
        match cmd {
            ID_SETTINGS => emit(BeaverEvent::OpenSettings),
            ID_HIDE => {
                set_visible(false);
                emit(BeaverEvent::Hidden);
            }
            id => {
                if let Some(&(_, _, px)) = SIZES.iter().find(|s| s.0 == id) {
                    LOGICAL.store(px, SeqCst);
                    refit();
                    let mut r = RECT::default();
                    let _ = GetWindowRect(hwnd(), &mut r);
                    emit(BeaverEvent::Moved(r.left, r.top));
                    emit(BeaverEvent::Resized(px));
                }
            }
        }
    }
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    crate::crash::guarded("beaver window", || unsafe { wndproc_body(h, msg, wp, lp) }).unwrap_or(LRESULT(0))
}

unsafe fn wndproc_body(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            // Grab anywhere on the icon to move it.
            WM_NCHITTEST => LRESULT(HTCAPTION as isize),
            // The glass follows the screen behind it, live while dragging.
            WM_MOVE => {
                repaint();
                LRESULT(0)
            }
            WM_TIMER if wp.0 == TIMER_REFRESH => {
                if IsWindowVisible(h).as_bool() {
                    repaint();
                }
                LRESULT(0)
            }
            // Picked up: redraw the desktop it will glide over (desktop icons may have moved).
            WM_ENTERSIZEMOVE => {
                let _ = crate::capture::desktop_rgba(0, 0, 1, 1, true);
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                refit();
                let mut r = RECT::default();
                let _ = GetWindowRect(h, &mut r);
                emit(BeaverEvent::Moved(r.left, r.top));
                LRESULT(0)
            }
            WM_NCLBUTTONDBLCLK => {
                emit(BeaverEvent::OpenSettings);
                LRESULT(0)
            }
            // With HTCAPTION, right-clicks arrive as non-client clicks; show our menu
            // instead of the system one.
            WM_NCRBUTTONUP | WM_CONTEXTMENU => {
                let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
                context_menu(x, y);
                LRESULT(0)
            }
            WM_NCRBUTTONDOWN => LRESULT(0),
            WM_APP_RAISE => {
                if wp.0 != 0 {
                    SetWindowLongPtrW(h, GWLP_HWNDPARENT, 0);
                    let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                } else {
                    sink(h);
                }
                LRESULT(0)
            }
            WM_APP_VISIBLE => {
                let _ = ShowWindow(h, if wp.0 != 0 { SW_SHOWNOACTIVATE } else { SW_HIDE });
                LRESULT(0)
            }
            _ => DefWindowProcW(h, msg, wp, lp),
        }
    }
}

#[implement(IDropTarget)]
struct BeaverDrop {
    perked: Cell<bool>,
}

impl BeaverDrop_Impl {
    fn perk(&self, on: bool) {
        if self.perked.replace(on) != on {
            unsafe { paint(if on { PERKED } else { REST }) };
        }
    }
}

/// The beaver itself never accepts the drop: it opens the ring, whose drop target sits
/// on top and takes the drag from there.
impl IDropTarget_Impl for BeaverDrop_Impl {
    fn DragEnter(&self, obj: Option<&IDataObject>, _k: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        crate::crash::guarded("beaver drag enter", || {
            let files = obj.map(|o| unsafe { o.QueryGetData(&crate::trigger::hdrop_format()) == S_OK }).unwrap_or(false);
            log::debug!("beaver drag enter, files={files}");
            if files {
                self.perk(true);
                let mut r = RECT::default();
                unsafe {
                    let _ = GetWindowRect(hwnd(), &mut r);
                }
                crate::trigger::open_ring_at((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            }
        });
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }

    fn DragOver(&self, _k: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        crate::crash::guarded("beaver drag leave", || self.perk(false));
        Ok(())
    }

    fn Drop(&self, _obj: Option<&IDataObject>, _k: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        crate::crash::guarded("beaver drop", || self.perk(false));
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }
}

/// Where the icon starts out: the main screen's bottom-right corner, as (x, y, size).
unsafe fn home() -> (i32, i32, i32) {
    unsafe {
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let size = size_at(POINT { x: work.right - 1, y: work.bottom - 1 });
        let margin = (MARGIN_LOGICAL * size as f64 / LOGICAL.load(SeqCst) as f64) as i32;
        (work.right - size - margin, work.bottom - size - margin, size)
    }
}

/// Puts the icon back in its corner, visible, at desktop level and freshly drawn: the way
/// out when it seems stuck. Call on the trigger thread.
pub(crate) unsafe fn reset() {
    unsafe {
        if HWND_RAW.load(SeqCst) == 0 {
            return;
        }
        let h = hwnd();
        let (x, y, size) = home();
        SIZE_PX.store(size, SeqCst);
        let _ = SetWindowPos(h, None, x, y, size, size, SWP_NOZORDER | SWP_NOACTIVATE);
        RAISED.store(false, SeqCst);
        sink(h);
        WANT_VISIBLE.store(true, SeqCst);
        let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        let _ = crate::capture::desktop_rgba(0, 0, 1, 1, true);
        DRAWN.store(0, SeqCst);
        paint(REST);
        log::info!("icon reset to ({x}, {y}), {size}px");
    }
}

/// Creates the icon on the calling (trigger) thread. `start` is where it was last left,
/// `logical` its size in logical px.
pub(crate) unsafe fn create(hinst: HMODULE, start: Option<(i32, i32)>, logical: i32) -> Result<()> {
    unsafe {
        let class = w!("ConvertWidget");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_HAND)?,
            style: CS_DBLCLKS,
            ..Default::default()
        };
        RegisterClassW(&wc);
        LOGICAL.store(if SIZES.iter().any(|s| s.2 == logical) { logical } else { 48 }, SeqCst);

        // Last position, if it's still on a connected monitor; else the main screen's
        // bottom-right corner.
        let on_screen = |(x, y): (i32, i32)| !MonitorFromPoint(POINT { x: x + 10, y: y + 10 }, MONITOR_DEFAULTTONULL).is_invalid();
        let (x, y, size) = match start.filter(|p| on_screen(*p)) {
            Some((x, y)) => (x, y, size_at(POINT { x, y })),
            None => home(),
        };
        SIZE_PX.store(size, SeqCst);
        let h = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("con/vert"),
            WS_POPUP,
            x,
            y,
            size,
            size,
            None,
            None,
            hinst,
            None,
        )?;
        HWND_RAW.store(h.0 as isize, SeqCst);
        sink(h);
        paint(REST);
        SetTimer(h, TIMER_REFRESH, REFRESH_MS, None);
        let target: IDropTarget = BeaverDrop { perked: Cell::new(false) }.into();
        RegisterDragDrop(h, &target)?;
        if WANT_VISIBLE.load(SeqCst) {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
        log::info!("icon ready at ({x}, {y}), {size}px");
        Ok(())
    }
}
