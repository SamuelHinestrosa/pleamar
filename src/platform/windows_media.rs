//! System media sessions and network status, without helper executables.
use super::SysValue;
use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status};
use windows::Networking::Connectivity::{NetworkInformation, NetworkConnectivityLevel};
use windows::core::Result;
use std::{sync::Mutex, time::{Duration, Instant}};

// A player can stop responding. Bound WinRT waits on the service worker so
// one missing reply cannot keep every later media command queued forever.
macro_rules! complete {
    ($operation:expr) => {
        (|| {
            let operation = $operation?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while operation.Status()?.0 == 0 {
                if Instant::now() >= deadline {
                    let _ = operation.Cancel();
                    return Err(windows::core::Error::new(windows::core::HRESULT(0x800705b4u32 as i32), "Windows media request timed out"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            operation.GetResults()
        })()
    };
}

static MANAGER: Mutex<Option<Manager>> = Mutex::new(None);

fn session() -> Result<Option<Session>> {
    let manager = {
        let mut manager = MANAGER.lock().unwrap();
        if manager.is_none() { *manager = Some(complete!(Manager::RequestAsync())?); }
        manager.as_ref().unwrap().clone()
    };
    let sessions = manager.GetSessions().inspect_err(|_| { *MANAGER.lock().unwrap() = None; })?;
    if sessions.Size()? == 0 { return Ok(None); }
    manager.GetCurrentSession().or_else(|_| sessions.GetAt(0)).map(Some)
}

pub(super) fn unavailable(error: &str) -> SysValue {
    SysValue::Map(vec![
        ("available".into(), SysValue::Bool(false)), ("error".into(), SysValue::Text(error.into())),
        ("player".into(), SysValue::Text(String::new())), ("playing".into(), SysValue::Bool(false)),
        ("title".into(), SysValue::Text(String::new())), ("artist".into(), SysValue::Text(String::new())),
        ("album".into(), SysValue::Text(String::new())),
        ("can_toggle".into(), SysValue::Bool(false)), ("can_next".into(), SysValue::Bool(false)),
        ("can_previous".into(), SysValue::Bool(false)),
    ])
}

#[derive(Default)]
struct Controls { playing: bool, toggle: bool, play: bool, pause: bool, next: bool, previous: bool }
#[derive(Debug, PartialEq)]
enum Action { Toggle, Play, Pause, Next, Previous }
impl Controls {
    fn read(session: &Session) -> Result<Self> {
        let playback = session.GetPlaybackInfo()?;
        let controls = playback.Controls()?;
        Ok(Self { playing: playback.PlaybackStatus()? == Status::Playing,
            toggle: controls.IsPlayPauseToggleEnabled()?, play: controls.IsPlayEnabled()?,
            pause: controls.IsPauseEnabled()?, next: controls.IsNextEnabled()?, previous: controls.IsPreviousEnabled()? })
    }
    fn action(&self, name: &str) -> std::result::Result<Action, String> {
        match name {
            "media.toggle" if self.toggle => Ok(Action::Toggle),
            "media.toggle" if self.playing && self.pause => Ok(Action::Pause),
            "media.toggle" if !self.playing && self.play => Ok(Action::Play),
            "media.play" if self.play => Ok(Action::Play),
            "media.pause" if self.pause => Ok(Action::Pause),
            "media.next" if self.next => Ok(Action::Next),
            "media.previous" if self.previous => Ok(Action::Previous),
            _ => Err(format!("the current media session does not support {name}")),
        }
    }
}

pub fn media() -> Result<SysValue> {
    let Some(session) = session()? else {
        return Ok(unavailable(""));
    };
    let properties = complete!(session.TryGetMediaPropertiesAsync())?;
    let controls = Controls::read(&session)?;
    Ok(SysValue::Map(vec![
        ("available".into(), SysValue::Bool(true)), ("error".into(), SysValue::Text(String::new())),
        ("player".into(), SysValue::Text(session.SourceAppUserModelId()?.to_string())),
        ("title".into(), SysValue::Text(properties.Title()?.to_string())),
        ("artist".into(), SysValue::Text(properties.Artist()?.to_string())),
        ("album".into(), SysValue::Text(properties.AlbumTitle()?.to_string())),
        ("playing".into(), SysValue::Bool(controls.playing)),
        ("can_toggle".into(), SysValue::Bool(controls.action("media.toggle").is_ok())),
        ("can_next".into(), SysValue::Bool(controls.next)), ("can_previous".into(), SysValue::Bool(controls.previous)),
    ]))
}

pub fn command(name: &str, args: &[SysValue]) -> std::result::Result<(), String> {
    if !matches!(name, "media.toggle" | "media.play" | "media.pause" | "media.next" | "media.previous") {
        return Err(format!("unsupported media command: {name}"));
    }
    let expected = match args {
        [] => None,
        [SysValue::Text(player)] if !player.is_empty() => Some(player),
        _ => return Err("media controls take an optional player identifier from media.state".into()),
    };
    let session = session().map_err(|e| e.to_string())?.ok_or("no Windows media session is active")?;
    if let Some(expected) = expected {
        if session.SourceAppUserModelId().map_err(|e| e.to_string())?.to_string() != *expected {
            return Err("the active media player changed; refresh before sending another command".into());
        }
    }
    let operation = match Controls::read(&session).map_err(|e| e.to_string())?.action(name)? {
        Action::Toggle => session.TryTogglePlayPauseAsync(),
        Action::Play => session.TryPlayAsync(),
        Action::Pause => session.TryPauseAsync(),
        Action::Next => session.TrySkipNextAsync(),
        Action::Previous => session.TrySkipPreviousAsync(),
    };
    if complete!(operation).map_err(|e| e.to_string())? { Ok(()) }
    else { Err(format!("the media player declined {name}")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toggle_falls_back_to_the_supported_playback_action() {
        let mut controls = Controls { playing: true, pause: true, ..Default::default() };
        assert_eq!(controls.action("media.toggle").unwrap(), Action::Pause);
        controls.playing = false;
        assert!(controls.action("media.toggle").is_err());
        controls.play = true;
        assert_eq!(controls.action("media.toggle").unwrap(), Action::Play);
        controls.toggle = true;
        assert_eq!(controls.action("media.toggle").unwrap(), Action::Toggle);
        assert!(controls.action("media.next").is_err());
        assert!(controls.action("media.previous").is_err());
        assert!(controls.action("media.pause_typo").is_err());
    }
    #[test]
    fn malformed_media_arguments_do_not_reach_a_player() {
        assert!(command("media.pause_typo", &[]).is_err());
        assert!(command("media.toggle", &[SysValue::Num(1.0)]).is_err());
        assert!(command("media.next", &[SysValue::Text(String::new())]).is_err());
    }

    #[test]
    #[ignore = "requires the owned media-fixture window and PLEAMAR_MEDIA_FIXTURE_PLAYER"]
    fn native_media_fixture_controls() {
        let _apartment = super::super::windows_system::Apartment::new().unwrap();
        let player = std::env::var("PLEAMAR_MEDIA_FIXTURE_PLAYER").expect("start the owned media-fixture first");
        assert!(player.starts_with("org.pleamar.validation.media."));
        let read = || {
            let SysValue::Map(fields) = media().unwrap() else { panic!("not a media snapshot") };
            let fields: std::collections::HashMap<_, _> = fields.into_iter().collect();
            assert_eq!(fields["player"], SysValue::Text(player.clone()), "refusing to control another player");
            fields
        };
        let await_value = |field: &str, expected: SysValue| {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let snapshot = read();
                if snapshot.get(field) == Some(&expected) { break; }
                assert!(Instant::now() < deadline, "media state did not confirm {field}: {snapshot:?}");
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let args = [SysValue::Text(player.clone())];
        assert_eq!(read()["can_previous"], SysValue::Bool(false));
        assert!(command("media.previous", &args).is_err());
        assert!(command("media.pause", &[SysValue::Text(format!("{player}-gone"))]).is_err());
        command("media.toggle", &args).unwrap();
        await_value("playing", SysValue::Bool(false));
        command("media.play", &args).unwrap();
        await_value("playing", SysValue::Bool(true));
        command("media.next", &args).unwrap();
        await_value("title", SysValue::Text("Pleamar media validation — 日本語".into()));
        await_value("can_next", SysValue::Bool(false));
        assert!(command("media.next", &args).is_err());
        command("media.previous", &args).unwrap();
        await_value("title", SysValue::Text("Pleamar media validation — Español".into()));
        println!("PASS: owned native media toggle/play/next/previous, Unicode, disabled capabilities and stale-player refusal");
    }
}

pub fn network() -> Result<SysValue> {
    let profile = match NetworkInformation::GetInternetConnectionProfile() {
        Ok(p) => p,
        Err(e) if e.code() == windows::core::HRESULT(0x80004003u32 as i32) => {
            // WinRT returned a null profile: there is no preferred connection.
            return Ok(SysValue::Map(vec![("online".into(), SysValue::Bool(false)), ("kind".into(), SysValue::Text("none".into())), ("name".into(), SysValue::Text(String::new()))]));
        }
        Err(e) => return Err(e),
    };
    let wifi = profile.IsWlanConnectionProfile()?;
    let mut result = vec![
        ("online".into(), SysValue::Bool(profile.GetNetworkConnectivityLevel()? != NetworkConnectivityLevel::None)),
        ("kind".into(), SysValue::Text(if wifi { "wifi" } else { "wired" }.into())),
        ("name".into(), SysValue::Text(profile.ProfileName()?.to_string())),
    ];
    if let Ok(bars) = profile.GetSignalBars().and_then(|b| b.Value()) {
        result.push(("strength".into(), SysValue::Num(bars as f64 / 5.0)));
    }
    Ok(SysValue::Map(result))
}
