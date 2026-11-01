//! End-to-end check with a real Explorer drag (Windows 11 runs it on a thread of its own):
//! opens `folder` in Explorer, drags `name` out of it with the mouse and
//! 1. taps Shift: the ring must open (and Shift again flips it);
//! 2. moves the floating icon, then drags the file onto it: the ring must open;
//! 3. moves the icon again and right away Shift-drags: the ring must still open.
//! Every drag is cancelled with Esc, nothing is converted. Prints PASS/FAIL per step.
//! usage: explorer_drag_test <folder> <file name in it>

use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn mouse(flags: MOUSE_EVENT_FLAGS, x: i32, y: i32) {
    unsafe {
        let (vx, vy) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (vw, vh) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: (x - vx) * 65535 / (vw - 1),
                    dy: (y - vy) * 65535 / (vh - 1),
                    dwFlags: flags | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE | MOUSEEVENTF_VIRTUALDESK,
                    ..Default::default()
                },
            },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

fn tap(vk: VIRTUAL_KEY) {
    unsafe {
        for up in [false, true] {
            let input = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } },
            };
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(50));
        }
    }
}

fn glide(from: (i32, i32), to: (i32, i32)) {
    for i in 1..=20 {
        mouse(MOUSE_EVENT_FLAGS(0), from.0 + (to.0 - from.0) * i / 20, from.1 + (to.1 - from.1) * i / 20);
        sleep(Duration::from_millis(20));
    }
}

fn rect_of(h: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(h, &mut r);
    }
    r
}

fn center(r: RECT) -> (i32, i32) {
    ((r.left + r.right) / 2, (r.top + r.bottom) / 2)
}

fn ring_visible() -> bool {
    unsafe { FindWindowW(None, w!("con/vert menu")).is_ok_and(|h| IsWindowVisible(h).as_bool()) }
}

/// Waits up to `ms` for the ring to show.
fn ring_within(ms: u64) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        if ring_visible() {
            return true;
        }
        sleep(Duration::from_millis(30));
    }
    false
}

/// Class and owner title of the window at a screen point.
fn top_class(p: (i32, i32)) -> String {
    unsafe {
        let h = WindowFromPoint(POINT { x: p.0, y: p.1 });
        let root = GetAncestor(h, GA_ROOT);
        let (mut c, mut t) = ([0u16; 64], [0u16; 64]);
        let n = GetClassNameW(h, &mut c) as usize;
        let m = GetWindowTextW(root, &mut t) as usize;
        format!("{} in \"{}\"", String::from_utf16_lossy(&c[..n]), String::from_utf16_lossy(&t[..m]))
    }
}

fn report(step: &str, ok: bool) {
    println!("{} {step}", if ok { "PASS" } else { "FAIL" });
}

/// The Explorer window showing `folder` (its title is the folder's name).
fn explorer_window(title: &str) -> Option<HWND> {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(8) {
        let mut found = None;
        let mut after = HWND::default();
        unsafe {
            while let Ok(w) = FindWindowExW(None, after, w!("CabinetWClass"), None) {
                let mut buf = [0u16; 260];
                let n = GetWindowTextW(w, &mut buf) as usize;
                if String::from_utf16_lossy(&buf[..n]).contains(title) {
                    found = Some(w);
                }
                after = w;
            }
        }
        if found.is_some() {
            return found;
        }
        sleep(Duration::from_millis(200));
    }
    None
}

/// Screen rect of the list item named `name` in Explorer window `win`.
fn item_rect(win: HWND, name: &str) -> Option<RECT> {
    unsafe {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(8) {
            if let Ok(root) = uia.ElementFromHandle(win) {
                let cond = uia.CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(BSTR::from(name))).ok()?;
                if let Ok(e) = root.FindFirst(TreeScope_Descendants, &cond) {
                    if let Ok(r) = e.CurrentBoundingRectangle() {
                        if r.right > r.left {
                            return Some(r);
                        }
                    }
                }
            }
            sleep(Duration::from_millis(250));
        }
        None
    }
}

/// Presses on the file and drags it `by` away, button still held.
fn grab(file: (i32, i32), by: (i32, i32)) -> (i32, i32) {
    mouse(MOUSE_EVENT_FLAGS(0), file.0, file.1);
    sleep(Duration::from_millis(150));
    mouse(MOUSEEVENTF_LEFTDOWN, file.0, file.1);
    sleep(Duration::from_millis(120));
    let to = (file.0 + by.0, file.1 + by.1);
    glide(file, to);
    sleep(Duration::from_millis(150));
    to
}

/// Cancels the drag (Esc) and lets go.
fn cancel(at: (i32, i32)) {
    tap(VK_ESCAPE);
    sleep(Duration::from_millis(100));
    mouse(MOUSEEVENTF_LEFTUP, at.0, at.1);
    sleep(Duration::from_millis(500));
}

fn move_icon(icon: HWND, by: (i32, i32)) {
    let from = center(rect_of(icon));
    mouse(MOUSE_EVENT_FLAGS(0), from.0, from.1);
    sleep(Duration::from_millis(100));
    mouse(MOUSEEVENTF_LEFTDOWN, from.0, from.1);
    sleep(Duration::from_millis(100));
    glide(from, (from.0 + by.0, from.1 + by.1));
    mouse(MOUSEEVENTF_LEFTUP, from.0 + by.0, from.1 + by.1);
    sleep(Duration::from_millis(60));
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let folder = args.next().expect("usage: explorer_drag_test <folder> <file name>");
    let name = args.next().expect("file name");
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let icon = unsafe { FindWindowW(w!("ConvertWidget"), None) }.expect("icon window not found (is the app running?)");
    let _ = std::process::Command::new("explorer.exe").arg(&folder).spawn();
    let title = std::path::Path::new(&folder).file_name().unwrap().to_string_lossy().into_owned();
    let win = explorer_window(&title).expect("Explorer window didn't open");
    unsafe {
        let _ = SetWindowPos(win, HWND_TOP, 100, 120, 900, 600, SWP_SHOWWINDOW);
        let _ = SetForegroundWindow(win);
    }
    sleep(Duration::from_millis(800));
    let file = center(item_rect(win, &name).expect("file not found in the Explorer window"));
    println!("file at {file:?}, icon at {:?}", center(rect_of(icon)));

    // A click on the file first, as a hand would: the first press in a just-opened window
    // only activates it.
    mouse(MOUSE_EVENT_FLAGS(0), file.0, file.1);
    mouse(MOUSEEVENTF_LEFTDOWN, file.0, file.1);
    mouse(MOUSEEVENTF_LEFTUP, file.0, file.1);
    sleep(Duration::from_millis(600));

    // ---- 1. Shift during an Explorer drag ----
    let at = grab(file, (450, 60));
    tap(VK_SHIFT);
    // A hand keeps moving a little.
    glide(at, (at.0 + 20, at.1 + 10));
    let at = (at.0 + 20, at.1 + 10);
    report("1a. Shift during an Explorer file drag opens the ring", ring_within(1200));
    cancel(at);
    report("1b. Esc closes it", !ring_visible());
    // What sits on top of the icon now? Must be the icon itself.
    let ic = center(rect_of(icon));
    let top = unsafe { WindowFromPoint(POINT { x: ic.0, y: ic.1 }) };
    let mut name = [0u16; 64];
    let n = unsafe { GetClassNameW(top, &mut name) } as usize;
    let class = String::from_utf16_lossy(&name[..n]);
    let overlay_up = unsafe { FindWindowW(w!("ConvertDropTarget"), None) }.is_ok_and(|h| unsafe { IsWindowVisible(h) }.as_bool());
    println!("   on top of the icon: {class}; drop target visible: {overlay_up}");
    report("1c. nothing left covering the icon", class == "ConvertWidget");

    // ---- 2. move the icon, then drag the file onto it ----
    let before = center(rect_of(icon));
    move_icon(icon, (-120, -40));
    let target = center(rect_of(icon));
    println!("   icon moved {before:?} -> {target:?}; on top there: {}", top_class(target));
    mouse(MOUSE_EVENT_FLAGS(0), file.0, file.1);
    sleep(Duration::from_millis(100));
    mouse(MOUSEEVENTF_LEFTDOWN, file.0, file.1);
    sleep(Duration::from_millis(120));
    glide(file, (file.0 + 60, file.1));
    glide((file.0 + 60, file.1), target);
    report("2. right after moving the icon, a file dragged onto it opens the ring", ring_within(1500));
    cancel(target);

    // ---- 3. move the icon again, then at once Shift-drag ----
    move_icon(icon, (120, 40));
    let at = grab(file, (450, -40));
    tap(VK_SHIFT);
    let opened = ring_within(1200);
    report("3a. right after moving the icon, Shift-drag opens the ring", opened);
    if opened {
        tap(VK_SHIFT);
        sleep(Duration::from_millis(300));
        report("3b. Shift again keeps the ring open on its other page", ring_visible());
    }
    cancel(at);

    // ---- 4. move the icon, drag the file onto it, Shift twice (log: ring page 1, 0) ----
    move_icon(icon, (-120, -40));
    let target = center(rect_of(icon));
    mouse(MOUSE_EVENT_FLAGS(0), file.0, file.1);
    sleep(Duration::from_millis(100));
    mouse(MOUSEEVENTF_LEFTDOWN, file.0, file.1);
    sleep(Duration::from_millis(120));
    glide(file, (file.0 + 60, file.1));
    glide((file.0 + 60, file.1), target);
    let opened = ring_within(1500);
    sleep(Duration::from_millis(500));
    println!("4. shift taps now");
    tap(VK_SHIFT);
    sleep(Duration::from_millis(400));
    tap(VK_SHIFT);
    sleep(Duration::from_millis(400));
    report("4a. after moving the icon, a ring opened from it stays open through Shift", opened && ring_visible());
    // Move about over the ring, as a hand looking for a petal does.
    let ring = unsafe { FindWindowW(None, w!("con/vert menu")) }.ok().map(rect_of);
    if let Some(r) = ring {
        let c = center(r);
        glide(target, (c.0, c.1 - (r.bottom - r.top) / 3));
        sleep(Duration::from_millis(300));
        report("4b. moving over the petals keeps it open", ring_visible());
        // ---- 5. let go on the icon, click the hub (log: ring page) ----
        glide((c.0, c.1 - (r.bottom - r.top) / 3), target);
        sleep(Duration::from_millis(200));
    }
    mouse(MOUSEEVENTF_LEFTUP, target.0, target.1);
    sleep(Duration::from_millis(700));
    report("5a. let go on the icon: ring waits for a click", ring_visible());
    println!("5. hub click now");
    mouse(MOUSEEVENTF_LEFTDOWN, target.0, target.1);
    sleep(Duration::from_millis(60));
    mouse(MOUSEEVENTF_LEFTUP, target.0, target.1);
    sleep(Duration::from_millis(400));
    report("5b. clicking the icon flips the page (ring still open)", ring_visible());
    tap(VK_ESCAPE);
    sleep(Duration::from_millis(500));
    move_icon(icon, (120, 40));
    unsafe {
        let _ = PostMessageW(win, WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    Ok(())
}
