//! Real desktop input, only on an explicitly opted-in disposable CI runner.
use super::*;
use std::{path::{Path, PathBuf}, process::{Child, Command}, os::windows::process::CommandExt};
use windows::{core::{w, HSTRING}, Win32::System::LibraryLoader::GetModuleHandleW};
fn field<'a>(value: &'a SysValue, name: &str) -> &'a SysValue {
    let SysValue::Map(entries) = value else { panic!("expected a map") };
    &entries.iter().find(|(key, _)| key == name).unwrap().1
}
fn text(value: &SysValue) -> &str { let SysValue::Text(value) = value else { panic!("expected text") }; value }
fn list(value: &SysValue) -> &[SysValue] { let SysValue::List(value) = value else { panic!("expected list") }; value }
fn command(name: &str, args: &[SysValue]) -> Result<(), String> {
    let mut values = vec![SysValue::Num(EPOCH.load(Ordering::Acquire) as f64)]; values.extend_from_slice(args);
    super::command(name, &values)
}
fn state(folder: &Path) -> serde_json::Value { std::fs::read(folder.join("state.json")).ok().and_then(|v| serde_json::from_slice(&v).ok()).unwrap_or(serde_json::Value::Null) }
fn wait(mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() { assert!(started.elapsed() < Duration::from_secs(12), "fixture timeout"); std::thread::sleep(Duration::from_millis(25)); }
}
fn request(folder: &Path, command: &str) {
    std::fs::write(folder.join("control"), command).unwrap();
    wait(|| state(folder)["command"] == command);
}
struct OwnedChild(Child);
impl Drop for OwnedChild { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
fn catalog_id(title: &str) -> String {
    let catalog = query("desktop.windows", &[]).unwrap();
    text(field(list(field(&catalog,"windows")).iter().find(|entry| text(field(entry,"title")) == title).expect("owned fixture was not cataloged"), "id")).into()
}
fn picture(id: &str, output: &Path) -> image::RgbaImage {
    let result = query("desktop.look", &[SysValue::Text(id.into())]).unwrap();
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD.decode(text(field(&result,"data"))).unwrap();
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output).unwrap();
    std::io::Write::write_all(&mut file, &png).unwrap();
    image::load_from_memory(&png).unwrap().to_rgba8()
}

use windows::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Clone, Default)]
struct Observed {
    clicks: u32,
    right_clicks: u32,
    middle_clicks: u32,
    wheel: i32,
    horizontal_wheel: i32,
    dragging: bool,
    drags: u32,
    moves: u32,
    x: i32,
    y: i32,
    offset_x: i32,
    offset_y: i32,
    cancelled: bool,
}
thread_local! { static OBSERVED: RefCell<Observed> = RefCell::new(Observed { x: 20, y: 20, ..Observed::default() }); }

unsafe extern "system" fn input_procedure(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let x = (lparam.0 as u16) as i16 as i32;
        let y = ((lparam.0 >> 16) as u16) as i16 as i32;
        match msg {
            WM_PAINT => {
                let observed = OBSERVED.with(|value| value.borrow().clone());
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut client = RECT::default();
                let _ = GetClientRect(hwnd, &mut client);
                let blue = CreateSolidBrush(COLORREF(0x00904020));
                FillRect(dc, &client, blue);
                let _ = DeleteObject(blue.into());
                let red = CreateSolidBrush(COLORREF(0x003030e0));
                FillRect(dc, &RECT { left: observed.x, top: observed.y, right: observed.x + 110, bottom: observed.y + 65 }, red);
                let _ = DeleteObject(red.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, COLORREF(0x00ffffff));
                let label: Vec<_> = format!("Owned input fixture · clicks {} · wheel {} / {} · drags {}", observed.clicks, observed.wheel, observed.horizontal_wheel, observed.drags).encode_utf16().collect();
                let _ = TextOutW(dc, 20, 260, &label);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_COMMAND if wparam.0 & 0xffff == 42 && (wparam.0 >> 16) & 0xffff == 0 => {
                OBSERVED.with(|value| value.borrow_mut().clicks += 1);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let drag = OBSERVED.with(|value| {
                    let mut value = value.borrow_mut();
                    value.dragging = x >= value.x && x < value.x + 110 && y >= value.y && y < value.y + 65;
                    value.offset_x = x - value.x;
                    value.offset_y = y - value.y;
                    value.dragging
                });
                let _ = SetFocus(Some(hwnd));
                if drag { SetCapture(hwnd); }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let drag = OBSERVED.with(|value| {
                    let mut value = value.borrow_mut();
                    if value.dragging {
                        value.x = x - value.offset_x;
                        value.y = y - value.offset_y;
                        value.moves += 1;
                    }
                    value.dragging
                });
                if drag { let _ = InvalidateRect(Some(hwnd), None, false); }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let drag = OBSERVED.with(|value| {
                    let mut value = value.borrow_mut();
                    let drag = value.dragging;
                    if drag { value.drags += 1; }
                    value.dragging = false;
                    drag
                });
                if drag { let _ = ReleaseCapture(); }
                LRESULT(0)
            }
            WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                OBSERVED.with(|value| {
                    let mut value = value.borrow_mut();
                    if msg == WM_RBUTTONDOWN { value.right_clicks += 1; } else { value.middle_clicks += 1; }
                });
                LRESULT(0)
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let delta = ((wparam.0 >> 16) as u16) as i16 as i32;
                OBSERVED.with(|value| {
                    let mut value = value.borrow_mut();
                    if msg == WM_MOUSEWHEEL { value.wheel += delta; } else { value.horizontal_wheel += delta; }
                });
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 == 0x1b => {
                OBSERVED.with(|value| value.borrow_mut().cancelled = true);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn require_ci() {
    assert!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
        && std::env::var("RUNNER_ENVIRONMENT").as_deref() == Ok("github-hosted")
        && std::env::var("PLEAMAR_CI_DESKTOP_INPUT").as_deref() == Ok("1"),
        "real input requires the explicit disposable GitHub-hosted CI step");
}

fn evidence_directory(name: &str) -> PathBuf {
    require_ci();
    let root = PathBuf::from(std::env::var_os("RUNNER_TEMP").expect("runner temp directory")).canonicalize().unwrap();
    let folder = PathBuf::from(std::env::var_os(name).expect("owned evidence directory"));
    let parent = folder.parent().unwrap().canonicalize().unwrap();
    assert!(parent == root && folder.file_name().is_some(), "evidence must be a new direct child of runner temp");
    folder
}

fn ci_monitor() -> Monitor {
    require_ci();
    let screen = monitors().unwrap().into_iter().find(|m| m.primary).expect("CI primary output");
    assert!(screen.work.right - screen.work.left >= 900 && screen.work.bottom - screen.work.top >= 650,
        "owned CI fixtures do not fit on the disposable desktop");
    screen
}

#[test]
#[ignore = "owned interactive child fixture; never launch as an ordinary unit test"]
fn owned_input_fixture() {
    let folder = evidence_directory("PLEAMAR_OWNED_INPUT_FIXTURE");
    let parent: u32 = std::env::var("PLEAMAR_INPUT_TEST_PARENT").unwrap().parse().unwrap();
    let _dpi = Dpi::physical().unwrap();
    let screen = ci_monitor();
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("PleamarDesktopOwnedInputTest");
        let wc = WNDCLASSW { lpfnWndProc: Some(input_procedure), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
        assert_ne!(RegisterClassW(&wc), 0);
        let title = HSTRING::from(format!("Pleamar input fixture {} — Español 日本語", std::process::id()));
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, &title, WS_OVERLAPPEDWINDOW,
            screen.work.left + 80, screen.work.top + 90, 560, 420, None, None, Some(instance.into()), None).unwrap();
        let edit = CreateWindowExW(WS_EX_CLIENTEDGE, w!("EDIT"), w!(""), WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            20, 180, 480, 40, Some(hwnd), Some(HMENU(41usize as _)), Some(instance.into()), None).unwrap();
        let _button = CreateWindowExW(WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Count a click"), WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            320, 25, 170, 60, Some(hwnd), Some(HMENU(42usize as _)), Some(instance.into()), None).unwrap();
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let began = Instant::now();
        let mut previous_command = String::new();
        let mut previous_state = String::new();
        loop {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let control = std::fs::read_to_string(folder.join("control")).unwrap_or_default();
            if control != previous_command {
                match control.as_str() {
                    // The fixture currently owns focus. It may authorize only
                    // its test parent to simulate Marea's approval activation.
                    "allow-parent" => { AllowSetForegroundWindow(parent).unwrap(); }
                    "quit" => (),
                    _ => panic!("unknown input fixture command"),
                }
                previous_command = control.clone();
            }
            let observed = OBSERVED.with(|value| value.borrow().clone());
            let mut origin = POINT::default();
            assert!(ClientToScreen(hwnd, &mut origin).as_bool());
            let rect = bounds(hwnd).unwrap();
            let report = serde_json::json!({"ready":true,"command":control,"text":caption(edit),
                "clicks":observed.clicks,"right_clicks":observed.right_clicks,"middle_clicks":observed.middle_clicks,
                "wheel":observed.wheel,"horizontal_wheel":observed.horizontal_wheel,"drags":observed.drags,
                "moves":observed.moves,"dragging":observed.dragging,"patch":[observed.x,observed.y],
                "cancelled":observed.cancelled,"client":[origin.x-rect.left,origin.y-rect.top]});
            let serialized = report.to_string();
            if serialized != previous_state {
                std::fs::write(folder.join("state.json"), &serialized).unwrap();
                previous_state = serialized;
            }
            if control == "quit" || observed.cancelled || began.elapsed() > Duration::from_secs(120) { break; }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = DestroyWindow(hwnd);
    }
}

struct ApprovalWindow(HWND);
impl Drop for ApprovalWindow { fn drop(&mut self) { unsafe { let _ = DestroyWindow(self.0); } } }

fn activate_owned_approval(window: HWND) {
    let screen = ci_monitor();
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        assert_eq!(pid, GetCurrentProcessId());
        for key in [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN, VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_ESCAPE] {
            assert!(GetAsyncKeyState(key.0 as i32) >= 0, "release held input {:#x} before the owned input test", key.0);
        }
        let mut point = POINT { x: 60, y: 40 };
        assert!(ClientToScreen(window, &mut point).as_bool());
        assert!(point.x >= screen.work.left && point.x < screen.work.right && point.y >= screen.work.top && point.y < screen.work.bottom);
        assert_eq!(GetAncestor(WindowFromPoint(point), GA_ROOT), window, "approval bootstrap point is covered");
        input::cursor_clip(&[point]).expect("approval point is outside the user's permitted cursor area");
        let mut gui = GUITHREADINFO { cbSize:size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(0, &mut gui).unwrap();
        assert!(gui.hwndCapture.is_invalid(), "a window still owns mouse capture; do not bootstrap input");
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        assert!(width > 1 && height > 1);
        // A single test-driver click reproduces the user's approval activation.
        // All subsequent actions go through the production desktop service.
        let mouse = |flags, x, y| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx:x, dy:y, dwFlags:flags, ..Default::default() } } };
        let movement = mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                (((point.x-left) as i64*65536+32768)/width as i64) as i32,
                (((point.y-top) as i64*65536+32768)/height as i64) as i32);
        assert_eq!(SendInput(&[movement],size_of::<INPUT>() as i32),1);
        let mut actual = POINT::default();
        GetCursorPos(&mut actual).unwrap();
        assert_eq!(actual,point,"approval cursor movement was redirected; no click sent");
        input::cursor_clip(&[point]).unwrap();
        assert_eq!(GetAncestor(WindowFromPoint(actual), GA_ROOT), window);
        GetGUIThreadInfo(0, &mut gui).unwrap();
        assert!(gui.hwndCapture.is_invalid(), "capture changed before the approval click");
        let inputs = [mouse(MOUSEEVENTF_LEFTDOWN,0,0),mouse(MOUSEEVENTF_LEFTUP,0,0)];
        let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
        if sent == 1 { let _ = SendInput(&[mouse(MOUSEEVENTF_LEFTUP,0,0)], size_of::<INPUT>() as i32); }
        assert_eq!(sent, 2, "Windows rejected the owned approval click");
    }
    let began = Instant::now();
    while unsafe { GetForegroundWindow() } != window {
        assert!(began.elapsed() < Duration::from_secs(2), "owned approval did not acquire foreground focus");
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    pump();
}

fn assert_owned_target(id: &str, pid: u32) -> Entry {
    let screen = ci_monitor();
    let entry = target(id).unwrap();
    assert_eq!(entry.identity.process, pid);
    assert!(entry.rect.left >= screen.work.left && entry.rect.top >= screen.work.top
        && entry.rect.right <= screen.work.right && entry.rect.bottom <= screen.work.bottom, "owned target left the CI work area");
    entry
}

#[test]
#[ignore = "sends real input only to owned disposable CI fixtures; requires an idle keyboard/mouse"]
fn native_positive_input() {
    require_ci();
    let began = Instant::now();
    let _dpi = prepare().unwrap();
    let screen = ci_monitor();
    let folder = evidence_directory("PLEAMAR_DESKTOP_TEST_DIR");
    std::fs::create_dir(&folder).expect("input evidence needs a new directory");
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "platform::windows_desktop::ci_input_tests::owned_input_fixture", "--nocapture"])
        .env("PLEAMAR_OWNED_INPUT_FIXTURE", &folder)
        .env("PLEAMAR_INPUT_TEST_PARENT", std::process::id().to_string())
        .creation_flags(0x08000000 | 0x00004000).spawn().unwrap();
    let mut child = OwnedChild(child);
    wait(|| state(&folder)["ready"] == true);
    let name = format!("Pleamar input fixture {} — Español 日本語", child.0.id());
    let id = catalog_id(&name);
    let entry = assert_owned_target(&id, child.0.id());
    let approval = unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("PleamarOwnedApprovalFixture");
        let wc = WNDCLASSW { lpfnWndProc: Some(input_procedure), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
        assert_ne!(RegisterClassW(&wc), 0);
        ApprovalWindow(CreateWindowExW(WS_EX_TOPMOST, class, w!("Owned Marea approval fixture"), WS_OVERLAPPEDWINDOW,
            screen.work.right - 270, screen.work.top + 90, 240, 120, None, None, Some(instance.into()), None).unwrap())
    };
    unsafe { let _ = ShowWindow(approval.0, SW_SHOWNOACTIVATE); }
    pump();
    activate_owned_approval(approval.0);
    command("desktop.focus", &[SysValue::Text(id.clone())]).unwrap();
    wait(|| unsafe { GetForegroundWindow() } == entry.identity.window());
    let client = state(&folder)["client"].clone();
    let point = |x: i32, y: i32| [SysValue::Num((x + client[0].as_i64().unwrap() as i32) as f64), SysValue::Num((y + client[1].as_i64().unwrap() as i32) as f64)];
    let action = |label: &str, operation: &str, args: &[SysValue]| {
        assert_owned_target(&id, child.0.id());
        assert_ne!(state(&folder)["cancelled"], true);
        assert!(unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } >= 0, "owned input test stopped by Escape");
        let _ = picture(&id, &folder.join(format!("{label}.png")));
        let mut values = vec![SysValue::Text(id.clone())];
        values.extend_from_slice(args);
        command(operation, &values).unwrap();
    };
    let click = |label: &str, x: i32, y: i32, button: &str| {
        let mut args = point(x,y).to_vec();
        args.extend([SysValue::Text(button.into()), SysValue::Num(1.0)]);
        action(label, "desktop.click", &args);
    };
    click("01-button", 390, 50, "left");
    wait(|| state(&folder)["clicks"] == 1);
    click("02-edit", 130, 198, "left");
    let message = "Hola, España 🎵 日本語";
    action("03-unicode", "desktop.type", &[SysValue::Text(message.into())]);
    wait(|| state(&folder)["text"] == message);
    action("04-backspace", "desktop.key", &[SysValue::Text("backspace".into())]);
    wait(|| state(&folder)["text"] == "Hola, España 🎵 日本");
    action("05-select", "desktop.hotkey", &[SysValue::Text("ctrl+a".into())]);
    action("06-replace", "desktop.type", &[SysValue::Text("Nuevo: café ☕".into())]);
    wait(|| state(&folder)["text"] == "Nuevo: café ☕");

    request(&folder, "allow-parent");
    unsafe {
        let _ = ShowWindow(approval.0, SW_SHOWNOACTIVATE);
        assert!(SetForegroundWindow(approval.0).as_bool(), "Windows did not activate the owned approval fixture");
    }
    pump();
    assert_eq!(unsafe { GetForegroundWindow() }, approval.0);
    action("07-approval", "desktop.type", &[SysValue::Text(" + aprobado".into())]);
    wait(|| state(&folder)["text"] == "Nuevo: café ☕ + aprobado");
    assert_eq!(unsafe { GetForegroundWindow() }, entry.identity.window());
    drop(approval);

    click("08-canvas", 480, 320, "left");
    let mut wheel = point(480,320).to_vec();
    wheel.extend([SysValue::Text("up".into()), SysValue::Num(3.0)]);
    action("09-wheel", "desktop.scroll", &wheel);
    wait(|| state(&folder)["wheel"] == 360);
    wheel[2] = SysValue::Text("right".into()); wheel[3] = SysValue::Num(2.0);
    action("10-horizontal", "desktop.scroll", &wheel);
    wait(|| state(&folder)["horizontal_wheel"] == 240);
    let mut drag = point(30,30).to_vec(); drag.extend(point(180,100));
    action("11-drag", "desktop.drag", &drag);
    wait(|| state(&folder)["drags"] == 1);
    let dragged = state(&folder);
    assert_eq!(dragged["patch"], serde_json::json!([170,90]));
    assert_eq!(dragged["dragging"], false);
    assert!(dragged["moves"].as_u64().unwrap() > 0);
    click("12-right", 480, 320, "right");
    wait(|| state(&folder)["right_clicks"] == 1);
    click("13-middle", 480, 320, "middle");
    wait(|| state(&folder)["middle_clicks"] == 1);
    let _ = picture(&id, &folder.join("14-result.png"));
    let mut report = state(&folder);
    report["passed"] = serde_json::json!(true);
    report["environment"] = serde_json::json!("github-hosted");
    report["full_product_acceptance"] = serde_json::json!(false);
    report["monitor"] = serde_json::json!(screen.name);
    report["physical_input_sent"] = serde_json::json!(true);
    report["approval_focus_returned"] = serde_json::json!(true);
    report["bootstrap"] = serde_json::json!("one checked click on this test process's CI approval window");
    report["only_owned_targets"] = serde_json::json!(true);
    request(&folder,"quit");
    wait(|| child.0.try_wait().unwrap().is_some());
    assert!(child.0.try_wait().unwrap().unwrap().success());
    report["seconds"] = serde_json::json!(began.elapsed().as_secs_f64());
    std::fs::write(folder.join("result.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("positive native input evidence: {}", folder.display());
}
