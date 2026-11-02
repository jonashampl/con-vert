//! End-to-end checks against the running app for the problems found in daily use:
//! 1. a mouse drag that isn't a file drag (moving notes in FL Studio) + Shift: no ring;
//! 1b. a file drag from an app other than Explorer (FL Studio's browser) + Shift: no ring;
//! 2. move the icon, then right away drag a file onto it and hold still: the ring stays;
//! 3. Shift while still holding the file: the ring flips to its second page;
//! 4. let go on the icon, click the hub: flips again in click mode; Esc closes.
//! Prints PASS/FAIL per step; the app log ("ring page N") shows the flips.
//! usage: icon_flow_test <file>

use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[implement(IDropSource)]
struct Source;

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, esc: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if esc.as_bool() { DRAGDROP_S_CANCEL } else if keys.0 & MK_LBUTTON.0 == 0 { DRAGDROP_S_DROP } else { S_OK }
    }
    fn GiveFeedback(&self, _e: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

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

fn pump(ms: u64) {
    for _ in 0..(ms / 10) {
        unsafe {
            let mut m = MSG::default();
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).into() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        sleep(Duration::from_millis(10));
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

fn report(step: &str, ok: bool) {
    println!("{} {step}", if ok { "PASS" } else { "FAIL" });
}

unsafe extern "system" fn pad_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn main() -> Result<()> {
    let file = std::env::args().nth(1).expect("usage: icon_flow_test <file>");
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        OleInitialize(None)?;
        let b = FindWindowW(w!("ConvertWidget"), None).expect("icon window not found (is the app running?)");
        let home = center(rect_of(b));
        let mut affinity = 0u32;
        let _ = GetWindowDisplayAffinity(b, &mut affinity);
        report("0. icon shows up in screenshots and recordings (display affinity none)", affinity == WDA_NONE.0);
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSW { lpfnWndProc: Some(pad_proc), hInstance: hinst.into(), lpszClassName: w!("IconFlowPad"), hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0), ..Default::default() };
        RegisterClassW(&wc);
        let (px, py) = (home.0 - 900, home.1 - 600);
        let pad = CreateWindowExW(WS_EX_TOPMOST, w!("IconFlowPad"), w!("icon flow test"), WS_POPUP | WS_VISIBLE, px, py, 300, 300, None, None, hinst, None)?;
        let _ = SetForegroundWindow(pad);
        pump(300);
        let start = (px + 150, py + 150);

        // ---- 1. plain mouse drag on the pad + Shift: not a file drag, no ring ----
        mouse(MOUSE_EVENT_FLAGS(0), start.0, start.1);
        mouse(MOUSEEVENTF_LEFTDOWN, start.0, start.1);
        glide(start, (start.0 + 80, start.1 + 40));
        tap(VK_SHIFT);
        let t0 = Instant::now();
        let mut seen = false;
        while t0.elapsed() < Duration::from_millis(900) {
            seen |= ring_visible();
            sleep(Duration::from_millis(30));
        }
        mouse(MOUSEEVENTF_LEFTUP, start.0 + 80, start.1 + 40);
        report("1. Shift during a non-file drag shows no ring", !seen);
        pump(400);

        // ---- 1b. a file drag from another app (this one, not Explorer) + Shift: no ring ----
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(file.as_str()), None::<&IBindCtx>)?;
        let data: IDataObject = item.BindToHandler(None, &BHID_DataObject)?;
        let source: IDropSource = Source.into();
        mouse(MOUSE_EVENT_FLAGS(0), start.0, start.1);
        pump(100);
        mouse(MOUSEEVENTF_LEFTDOWN, start.0, start.1);
        pump(100);
        std::thread::spawn(move || {
            sleep(Duration::from_millis(200));
            glide(start, (start.0 + 80, start.1 + 40));
            tap(VK_SHIFT);
            let t0 = Instant::now();
            let mut seen = false;
            while t0.elapsed() < Duration::from_millis(900) {
                seen |= ring_visible();
                sleep(Duration::from_millis(30));
            }
            report("1b. Shift during a file drag from another app shows no ring", !seen);
            tap(VK_ESCAPE);
            mouse(MOUSEEVENTF_LEFTUP, start.0 + 80, start.1 + 40);
        });
        let mut effect = DROPEFFECT::default();
        let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_MOVE, &mut effect);
        pump(400);

        // ---- 2. move the icon, then at once drag a file onto it and hold still ----
        mouse(MOUSE_EVENT_FLAGS(0), home.0, home.1);
        sleep(Duration::from_millis(100));
        mouse(MOUSEEVENTF_LEFTDOWN, home.0, home.1);
        sleep(Duration::from_millis(100));
        let moved_to = (home.0 - 150, home.1 - 60);
        glide(home, moved_to);
        mouse(MOUSEEVENTF_LEFTUP, moved_to.0, moved_to.1);
        sleep(Duration::from_millis(150));
        let icon = center(rect_of(b));
        println!("icon moved to {icon:?}");

        mouse(MOUSE_EVENT_FLAGS(0), start.0, start.1);
        pump(100);
        mouse(MOUSEEVENTF_LEFTDOWN, start.0, start.1);
        pump(100);
        std::thread::spawn(move || {
            sleep(Duration::from_millis(200));
            glide(start, icon);
            // Hold perfectly still well past the old 0.8 s cut-off.
            sleep(Duration::from_millis(1800));
            report("2. ring stays open while the file is held still on the icon", ring_visible());
            // Snapshot for a look at how it draws (set RING_SHOT to a .png path).
            if let (Ok(out), Ok(ring)) = (std::env::var("RING_SHOT"), FindWindowW(None, w!("con/vert menu"))) {
                let r = rect_of(ring);
                let ff = std::env::var("CONVERT_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
                let _ = std::process::Command::new(ff)
                    .args(["-loglevel", "error", "-y", "-f", "gdigrab", "-offset_x", &r.left.to_string(), "-offset_y", &r.top.to_string(), "-video_size", &format!("{}x{}", r.right - r.left, r.bottom - r.top), "-i", "desktop", "-frames:v", "1", &out])
                    .status();
            }
            // ---- 3. Shift while holding: second page ----
            tap(VK_SHIFT);
            sleep(Duration::from_millis(400));
            report("3. ring still open after Shift (check log for 'ring page 1')", ring_visible());
            // Let go on the icon: click mode.
            mouse(MOUSEEVENTF_LEFTUP, icon.0, icon.1);
        });
        let mut effect = DROPEFFECT::default();
        let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_MOVE, &mut effect);
        pump(700);

        // ---- 4. click mode: click the hub flips, Esc closes ----
        report("4a. ring waits for a click after letting go on the icon", ring_visible());
        mouse(MOUSE_EVENT_FLAGS(0), icon.0, icon.1);
        sleep(Duration::from_millis(150));
        mouse(MOUSEEVENTF_LEFTDOWN, icon.0, icon.1);
        sleep(Duration::from_millis(60));
        mouse(MOUSEEVENTF_LEFTUP, icon.0, icon.1);
        sleep(Duration::from_millis(400));
        report("4b. ring still open after clicking the hub (log: 'ring page')", ring_visible());
        tap(VK_ESCAPE);
        sleep(Duration::from_millis(400));
        report("4c. Esc closes the ring", !ring_visible());

        // Put the icon back where it was.
        let now = center(rect_of(b));
        mouse(MOUSE_EVENT_FLAGS(0), now.0, now.1);
        sleep(Duration::from_millis(100));
        mouse(MOUSEEVENTF_LEFTDOWN, now.0, now.1);
        sleep(Duration::from_millis(100));
        glide(now, home);
        mouse(MOUSEEVENTF_LEFTUP, home.0, home.1);
        sleep(Duration::from_millis(300));
        let _ = DestroyWindow(pad);
    }
    Ok(())
}
