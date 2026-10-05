//! Windows input shares the user's foreground queue. Never claim to have a
//! separate keyboard or silently fall back to unacknowledged PostMessage input.
use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

fn string(v: &SysValue) -> Result<&str, String> { if let SysValue::Text(v) = v { Ok(v) } else { Err("expected text".into()) } }
fn number(v: &SysValue) -> Result<f64, String> { if let SysValue::Num(v) = v { if v.is_finite() { return Ok(*v); } } Err("expected a finite number".into()) }
fn integer(v: &SysValue, low: i32, high: i32) -> Result<i32, String> {
    let n = number(v)?;
    if n.fract() != 0.0 || n < low as f64 || n > high as f64 { return Err("integer argument is outside its supported range".into()); }
    Ok(n as i32)
}
fn point(x: &SysValue, y: &SysValue, rect: RECT) -> Result<POINT, String> {
    let (x, y) = (number(x)?, number(y)?);
    if x < 0.0 || y < 0.0 || x >= (rect.right - rect.left) as f64 || y >= (rect.bottom - rect.top) as f64 { return Err("the point is outside the last window picture".into()); }
    Ok(POINT { x: rect.left + x.floor() as i32, y: rect.top + y.floor() as i32 })
}
fn fresh(id: &str, entry: &Entry) -> Result<(), String> {
    CATALOG.with(|c| {
        let c = c.borrow();
        let shot = c.shots.get(id).ok_or("desktop.look must precede input")?;
        if shot.created.elapsed() > Duration::from_secs(30) || shot.target != entry.identity || shot.rect != entry.rect || shot.epoch != EPOCH.load(Ordering::Acquire) { return Err("the window or its picture changed; look again before acting".into()); }
        Ok(())
    })
}
fn keyboard_ready(entry: &Entry) -> Result<(), String> { unsafe {
    check_active()?;
    if !entry.identity.current() { return Err("the input window closed".into()); }
    if !IsWindowEnabled(entry.identity.window()).as_bool() { return Err("the window is blocked by a dialog; list and look again".into()); }
    // Do not release keys the user is holding, or mix their input with an
    // automation gesture. Retry only after they have released those keys.
    for key in [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN, VK_LBUTTON, VK_RBUTTON, VK_MBUTTON] {
        if GetAsyncKeyState(key.0 as i32) < 0 { return Err("the user is holding a modifier or mouse button; no input was sent".into()); }
    }
    if GetForegroundWindow() != entry.identity.window() {
        let mut foreground_pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut foreground_pid));
        // Clicking Marea's approval card may activate Marea. Return focus to
        // the explicitly approved target, but never steal it from a different
        // application the user has switched to while the model was thinking.
        if foreground_pid != GetCurrentProcessId() || !SetForegroundWindow(entry.identity.window()).as_bool()
            || GetForegroundWindow() != entry.identity.window() {
            return Err("Windows input requires this window in the foreground; request desktop_focus first".into());
        }
    }
    Ok(())
} }
fn key(key: VIRTUAL_KEY, up: bool, unicode: Option<u16>) -> INPUT {
    let extended = matches!(key, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_DELETE | VK_INSERT);
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT {
        wVk: if unicode.is_some() { VIRTUAL_KEY(0) } else { key }, wScan: unicode.unwrap_or(0),
        dwFlags: (if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }) | if unicode.is_some() { KEYEVENTF_UNICODE } else if extended { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) },
        ..Default::default() } } }
}
fn named_key(name: &str) -> Option<VIRTUAL_KEY> {
    Some(match name {
        "enter" => VK_RETURN, "tab" => VK_TAB, "escape" => VK_ESCAPE, "backspace" => VK_BACK,
        "space" => VK_SPACE, "up" => VK_UP, "down" => VK_DOWN, "left" => VK_LEFT, "right" => VK_RIGHT,
        "delete" => VK_DELETE, "home" => VK_HOME, "end" => VK_END, "pageup" => VK_PRIOR, "pagedown" => VK_NEXT,
        _ => { let n = name.strip_prefix('f')?.parse::<u16>().ok()?; if !(1..=12).contains(&n) { return None; } VIRTUAL_KEY(VK_F1.0 + n - 1) }
    })
}
fn hotkey(value: &str) -> Result<Vec<INPUT>, String> {
    if value.len() > 32 { return Err("shortcut is too long".into()); }
    let lower = value.to_ascii_lowercase();
    let parts: Vec<_> = lower.split('+').collect();
    if parts.len() < 2 || parts.len() > 4 { return Err("use a shortcut such as ctrl+shift+t".into()); }
    let mut mods = Vec::new();
    for name in &parts[..parts.len()-1] {
        let k = match *name { "ctrl" => VK_CONTROL, "alt" => VK_MENU, "shift" => VK_SHIFT, _ => return Err("supported modifiers are ctrl, alt and shift".into()) };
        if mods.contains(&k) { return Err("duplicate shortcut modifier".into()); }
        mods.push(k);
    }
    let name = parts[parts.len()-1];
    let k = named_key(name).or_else(|| if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() { Some(VIRTUAL_KEY(name.as_bytes()[0].to_ascii_uppercase() as u16)) } else { None }).ok_or("unknown shortcut key")?;
    let mut inputs: Vec<_> = mods.iter().map(|k| key(*k, false, None)).collect();
    inputs.extend([key(k, false, None), key(k, true, None)]);
    inputs.extend(mods.iter().rev().map(|k| key(*k, true, None)));
    Ok(inputs)
}
fn mouse(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: flags, mouseData: data as u32, ..Default::default() } } }
}
fn move_mouse(point: POINT) -> Result<INPUT, String> {
    let (left, top, width, height) = unsafe { (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN), GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN)) };
    if width < 2 || height < 2 || point.x < left || point.x >= left + width || point.y < top || point.y >= top + height { return Err("point is outside the connected desktop".into()); }
    let mut input = mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK, 0);
    input.Anonymous.mi.dx = (((point.x - left) as i64 * 65536 + 32768) / width as i64).min(65535) as i32;
    input.Anonymous.mi.dy = (((point.y - top) as i64 * 65536 + 32768) / height as i64).min(65535) as i32;
    Ok(input)
}
fn unobstructed(entry: &Entry, point: POINT) -> Result<(), String> {
    let hit = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
    if hit != entry.identity.window() { return Err("another window covers this point; no input was sent".into()); }
    Ok(())
}
fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) } as usize;
    if sent == inputs.len() { return Ok(()); }
    // Partial insertion must not leave our modifiers/buttons held down.
    let mut release = Vec::new();
    for input in &inputs[..sent] { unsafe {
        if input.r#type == INPUT_KEYBOARD && !input.Anonymous.ki.dwFlags.contains(KEYEVENTF_KEYUP) {
            let mut up = *input; up.Anonymous.ki.dwFlags |= KEYEVENTF_KEYUP; release.push(up);
        } else if input.r#type == INPUT_MOUSE {
            for (down, up) in [(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP), (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP), (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP)] {
                if input.Anonymous.mi.dwFlags.contains(down) { release.push(mouse(up, 0)); }
            }
        }
    } }
    if !release.is_empty() { unsafe { SendInput(&release, size_of::<INPUT>() as i32); } }
    Err(format!("Windows inserted {sent}/{} input events; the action may be partial (protected/elevated windows can reject input). Look again before retrying.", inputs.len()))
}
pub(super) fn command(name: &str, args: &[SysValue], epoch: u64) -> Result<(), String> {
    let (value, args) = args.split_first().ok_or("desktop action needs a catalog window id")?;
    let id = super::id(value)?;
    if name == "desktop.focus" && args.is_empty() {
        let identity = CATALOG.with(|c| c.borrow().entries.get(&id).map(|e| e.identity)).ok_or("unknown window")?;
        if !identity.current() { return Err("the window closed".into()); }
        check_epoch(epoch)?;
        unsafe {
            if IsIconic(identity.window()).as_bool() && !ShowWindowAsync(identity.window(), SW_RESTORE).as_bool() { return Err("Windows rejected restoring the window".into()); }
            if !SetForegroundWindow(identity.window()).as_bool() { return Err("Windows kept focus with the current application; select the target window yourself".into()); }
        }
        return Ok(());
    }
    let entry = target(&id)?;
    if name == "desktop.send" {
        let [index] = args else { return Err("desktop.send takes a monitor number".into()); };
        let screens = monitors()?;
        let monitor = screens.get(integer(index, 0, 63)? as usize).ok_or("the monitor is disconnected")?;
        let width = (entry.rect.right - entry.rect.left).min(monitor.work.right - monitor.work.left);
        let height = (entry.rect.bottom - entry.rect.top).min(monitor.work.bottom - monitor.work.top);
        check_epoch(epoch)?;
        unsafe { SetWindowPos(entry.identity.window(), None, monitor.work.left + ((monitor.work.right-monitor.work.left)-width)/2,
            monitor.work.top + ((monitor.work.bottom-monitor.work.top)-height)/2, width, height, SWP_NOACTIVATE | SWP_NOZORDER | SWP_ASYNCWINDOWPOS) }.map_err(|e| e.to_string())?;
        CATALOG.with(|c| { c.borrow_mut().shots.remove(&id); });
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            check_active()?;
            if !entry.identity.current() { return Err("the window closed while moving".into()); }
            if monitor_info(unsafe { MonitorFromWindow(entry.identity.window(), MONITOR_DEFAULTTONULL) }).is_some_and(|m| m.name == monitor.name) { return Ok(()); }
            std::thread::sleep(Duration::from_millis(20));
        }
        return Err("the application did not move to the requested monitor".into());
    }
    fresh(&id, &entry)?;
    let mut inputs = Vec::new();
    let mut points = Vec::new();
    match (name, args) {
        ("desktop.type", [text]) => {
            let text = string(text)?;
            if text.chars().count() > 4000 || text.contains('\0') { return Err("text exceeds 4000 characters or contains NUL".into()); }
            for c in text.encode_utf16() { inputs.extend([key(VIRTUAL_KEY(0), false, Some(c)), key(VIRTUAL_KEY(0), true, Some(c))]); }
        }
        ("desktop.key", [name]) => { let k = named_key(string(name)?).ok_or("unknown key")?; inputs.extend([key(k, false, None), key(k, true, None)]); }
        ("desktop.hotkey", [keys]) => inputs = hotkey(string(keys)?)?,
        ("desktop.click", [x, y, button, count]) => {
            let p = point(x, y, entry.rect)?;
            points.push(p);
            let (down, up) = match string(button)? { "left" => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP), "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP), "middle" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP), _ => return Err("unknown mouse button".into()) };
            inputs.push(move_mouse(p)?);
            for _ in 0..integer(count, 1, 3)? { inputs.extend([mouse(down, 0), mouse(up, 0)]); }
        }
        ("desktop.scroll", [x, y, direction, steps]) => {
            let p = point(x, y, entry.rect)?;
            points.push(p);
            let (flags, sign) = match string(direction)? { "up" => (MOUSEEVENTF_WHEEL, 1), "down" => (MOUSEEVENTF_WHEEL, -1), "left" => (MOUSEEVENTF_HWHEEL, -1), "right" => (MOUSEEVENTF_HWHEEL, 1), _ => return Err("unknown scroll direction".into()) };
            inputs.extend([move_mouse(p)?, mouse(flags, sign * integer(steps, 1, 30)? * 120)]);
        }
        ("desktop.drag", [x1, y1, x2, y2]) => {
            let a = point(x1, y1, entry.rect)?; let b = point(x2, y2, entry.rect)?;
            points.extend([a,b]);
            inputs.extend([move_mouse(a)?, mouse(MOUSEEVENTF_LEFTDOWN, 0)]);
            for i in 1..=24 {
                let p = POINT { x: a.x + (b.x-a.x)*i/24, y: a.y+(b.y-a.y)*i/24 };
                points.push(p);
                inputs.push(move_mouse(p)?);
            }
            inputs.push(mouse(MOUSEEVENTF_LEFTUP, 0));
        }
        _ => return Err("unknown desktop action or invalid arguments".into()),
    }
    check_epoch(epoch)?;
    keyboard_ready(&entry)?;
    let current = target(&id)?;
    fresh(&id, &current)?;
    for p in points { unobstructed(&current, p)?; }
    check_epoch(epoch)?;
    if inputs.is_empty() { return Ok(()); }
    // One insertion prevents another injected gesture interleaving with ours.
    let result = send(&inputs);
    CATALOG.with(|c| { c.borrow_mut().shots.remove(&id); });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn physical_points_respect_negative_monitor_origins_and_edges() {
        let rect = RECT { left: -1920, top: -180, right: 0, bottom: 900 };
        assert_eq!(point(&SysValue::Num(20.9), &SysValue::Num(40.2), rect).unwrap(), POINT { x: -1900, y: -140 });
        for x in [-1.0, 1920.0, f64::INFINITY, f64::NAN] { assert!(point(&SysValue::Num(x), &SysValue::Num(0.0), rect).is_err()); }
    }
    #[test] fn shortcuts_have_balanced_modifiers_and_reject_ambiguous_keys() {
        assert_eq!(hotkey("ctrl+shift+t").unwrap().len(), 6);
        assert_eq!(hotkey("alt+left").unwrap().len(), 4);
        for v in ["ctrl+ctrl+a", "win+r", "ctrl+", "alt+shift+ctrl+a+b", "ctrl+unknown"] { assert!(hotkey(v).is_err(), "{v}"); }
        let inputs = hotkey("ctrl+shift+t").unwrap();
        assert!(unsafe { inputs[5].Anonymous.ki.dwFlags.contains(KEYEVENTF_KEYUP) });
        assert_eq!(unsafe { inputs[5].Anonymous.ki.wVk }, VK_CONTROL);
    }
}
