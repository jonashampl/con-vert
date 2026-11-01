//! Drags the floating icon 120 px left and back, reporting whether it moved and what
//! window is under the cursor at each step.
//! usage: icon_move_probe

use std::thread::sleep;
use std::time::Duration;

use windows::core::*;
use windows::Win32::Foundation::*;
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

fn under(p: (i32, i32)) -> String {
    unsafe {
        let h = WindowFromPoint(POINT { x: p.0, y: p.1 });
        let mut c = [0u16; 64];
        let n = GetClassNameW(h, &mut c) as usize;
        let root = GetAncestor(h, GA_ROOT);
        let mut t = [0u16; 128];
        let m = GetWindowTextW(root, &mut t) as usize;
        let mut rc = [0u16; 64];
        let k = GetClassNameW(root, &mut rc) as usize;
        let mut pid = 0u32;
        GetWindowThreadProcessId(root, Some(&mut pid));
        let mut rr = RECT::default();
        let _ = GetWindowRect(root, &mut rr);
        format!("{} in {} \"{}\" pid {} rect {},{} {}x{} visible {}", String::from_utf16_lossy(&c[..n]), String::from_utf16_lossy(&rc[..k]), String::from_utf16_lossy(&t[..m]), pid, rr.left, rr.top, rr.right - rr.left, rr.bottom - rr.top, IsWindowVisible(root).as_bool())
    }
}

fn main() {
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let icon = unsafe { FindWindowW(w!("ConvertWidget"), None) }.expect("icon not found");
    for by in [-120, 120] {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(icon, &mut r);
        }
        let from = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        mouse(MOUSE_EVENT_FLAGS(0), from.0, from.1);
        sleep(Duration::from_millis(150));
        println!("under cursor before press: {}", under(from));
        mouse(MOUSEEVENTF_LEFTDOWN, from.0, from.1);
        sleep(Duration::from_millis(100));
        for i in 1..=20 {
            mouse(MOUSE_EVENT_FLAGS(0), from.0 + by * i / 20, from.1);
            sleep(Duration::from_millis(20));
        }
        mouse(MOUSEEVENTF_LEFTUP, from.0 + by, from.1);
        sleep(Duration::from_millis(200));
        let mut n = RECT::default();
        unsafe {
            let _ = GetWindowRect(icon, &mut n);
        }
        println!("moved {} -> {} ({})", r.left, n.left, if n.left != r.left { "OK" } else { "DID NOT MOVE" });
    }
}
