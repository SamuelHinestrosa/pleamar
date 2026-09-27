//! Sound, spoken to directly: PulseAudio's protocol over its socket, which
//! PipeWire speaks too (pipewire-pulse). Nothing is spawned: no `wpctl`, no
//! `pactl subscribe` underneath. One connection subscribes to the server's
//! events and reports on each; commands open their own, short, one.
//!
//! `{ volume, muted, input, input_muted, outputs, inputs }`: the default output
//! and input (volume as the mixer shows it, 0..1, 1 being 100 %), and the
//! devices for each, `{ { id, name, default }, … }` (the monitors of the
//! outputs are not inputs).

use super::SysValue;
use pulseaudio::protocol::{self, ChannelVolume, Command, SetDeviceMuteParams, SetDeviceVolumeParams, SinkInfo, SourceInfo, Volume};
use std::ffi::CString;
use std::io::BufReader;
use std::os::unix::net::UnixStream;

/// A connection to the sound server, authenticated and named.
struct Pulse {
    sock: BufReader<UnixStream>,
    version: u16,
    seq: u32,
}

type R<T> = Result<T, String>;

impl Pulse {
    fn connect(name: &str) -> R<Pulse> {
        let path = pulseaudio::socket_path_from_env().ok_or("no sound server")?;
        let mut sock = BufReader::new(UnixStream::connect(path).map_err(|e| e.to_string())?);
        let cookie = pulseaudio::cookie_path_from_env().and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
        let auth = protocol::AuthParams { version: protocol::MAX_VERSION, supports_shm: false, supports_memfd: false, cookie };
        protocol::write_command_message(sock.get_mut(), 0, &Command::Auth(auth), protocol::MAX_VERSION).map_err(|e| e.to_string())?;
        let (_, reply) = protocol::read_reply_message::<protocol::AuthReply>(&mut sock, protocol::MAX_VERSION).map_err(|e| e.to_string())?;
        let version = protocol::MAX_VERSION.min(reply.version);
        let mut p = Pulse { sock, version, seq: 1 };
        let mut props = protocol::Props::new();
        props.set(protocol::Prop::ApplicationName, CString::new(name).unwrap_or_default());
        p.ask::<protocol::SetClientNameReply>(Command::SetClientName(props))?;
        Ok(p)
    }

    fn send(&mut self, c: &Command) -> R<()> {
        self.seq += 1;
        protocol::write_command_message(self.sock.get_mut(), self.seq, c, self.version).map_err(|e| e.to_string())
    }

    fn ask<T: protocol::CommandReply>(&mut self, c: Command) -> R<T> {
        self.send(&c)?;
        protocol::read_reply_message::<T>(&mut self.sock, self.version).map(|(_, t)| t).map_err(|e| e.to_string())
    }

    fn ack(&mut self, c: Command) -> R<()> {
        self.send(&c)?;
        protocol::read_ack_message(&mut self.sock).map(|_| ()).map_err(|e| e.to_string())
    }

    fn state(&mut self) -> R<(protocol::ServerInfo, Vec<SinkInfo>, Vec<SourceInfo>)> {
        let server = self.ask::<protocol::ServerInfo>(Command::GetServerInfo)?;
        let sinks = self.ask::<protocol::SinkInfoList>(Command::GetSinkInfoList)?;
        let sources = self.ask::<protocol::SourceInfoList>(Command::GetSourceInfoList)?;
        Ok((server, sinks, sources))
    }
}

/// The volume as the mixer shows it: the loudest channel, 1 being 100 %.
fn level(v: &ChannelVolume) -> f64 {
    v.channels().iter().map(|c| c.as_u32()).max().unwrap_or(0) as f64 / Volume::NORM.as_u32() as f64
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn name_of(description: &Option<CString>, name: &CString) -> String {
    description.as_ref().unwrap_or(name).to_string_lossy().into_owned()
}

fn report(server: &protocol::ServerInfo, sinks: &[SinkInfo], sources: &[SourceInfo]) -> SysValue {
    let default_sink = server.default_sink_name.as_ref();
    let default_source = server.default_source_name.as_ref();
    let sink = sinks.iter().find(|s| Some(&s.name) == default_sink).or(sinks.first());
    let inputs: Vec<&SourceInfo> = sources.iter().filter(|s| s.monitor_of_sink_index.is_none()).collect();
    let source = inputs.iter().copied().find(|s| Some(&s.name) == default_source).or(inputs.first().copied());
    let device = |id: u32, name: String, default: bool| SysValue::Map(vec![("id".into(), SysValue::Num(id as f64)), ("name".into(), SysValue::Text(name)), ("default".into(), SysValue::Bool(default))]);
    SysValue::Map(vec![
        ("volume".into(), SysValue::Num(sink.map_or(0.0, |s| round(level(&s.cvolume))))),
        ("muted".into(), SysValue::Bool(sink.is_some_and(|s| s.muted))),
        // With no microphone the service exists all the same: what is missing is the input.
        ("input".into(), SysValue::Num(source.map_or(0.0, |s| round(level(&s.cvolume))))),
        ("input_muted".into(), SysValue::Bool(source.is_none_or(|s| s.muted))),
        ("outputs".into(), SysValue::List(sinks.iter().map(|s| device(s.index, name_of(&s.description, &s.name), Some(&s.name) == default_sink)).collect())),
        ("inputs".into(), SysValue::List(inputs.iter().map(|s| device(s.index, name_of(&s.description, &s.name), Some(&s.name) == default_source)).collect())),
    ])
}

/// Whether there is a sound server to speak to.
pub fn available() -> bool {
    Pulse::connect("pleamar").is_ok()
}

pub fn service(dispatch: Box<dyn Fn(SysValue) + Send>) -> bool {
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let mut last = String::new();
            loop {
                // Two connections: one hears the server's events, the other asks.
                let mut run = || -> R<()> {
                    let mut asker = Pulse::connect("pleamar")?;
                    let mut events = Pulse::connect("pleamar events")?;
                    events.ack(Command::Subscribe(protocol::SubscriptionMask::SINK | protocol::SubscriptionMask::SOURCE | protocol::SubscriptionMask::SERVER))?;
                    let mut tell = |asker: &mut Pulse| -> R<()> {
                        let (server, sinks, sources) = asker.state()?;
                        let v = report(&server, &sinks, &sources);
                        let fingerprint = format!("{v:?}");
                        if fingerprint != last {
                            last = fingerprint;
                            dispatch(v);
                        }
                        Ok(())
                    };
                    tell(&mut asker)?;
                    loop {
                        protocol::read_command_message(&mut events.sock, events.version).map_err(|e| e.to_string())?;
                        // A volume slider dragged is a stream of events: read what else
                        // is already waiting, and tell once.
                        events.sock.get_ref().set_nonblocking(true).ok();
                        while protocol::read_command_message(&mut events.sock, events.version).is_ok() {}
                        events.sock.get_ref().set_nonblocking(false).ok();
                        tell(&mut asker)?;
                    }
                };
                if let Err(e) = run() {
                    eprintln!("audio · the sound server went away ({e}): trying again");
                }
                // Restarted (or not up yet): ask again in a moment.
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        })
        .is_ok()
}

fn set_volume(p: &mut Pulse, sink: bool, index: u32, channels: usize, v: f64) -> R<()> {
    let mut cv = ChannelVolume::empty();
    let raw = Volume::from_u32_clamped((v.clamp(0.0, 1.0) * Volume::NORM.as_u32() as f64).round() as u32);
    for _ in 0..channels.max(1) {
        cv.push(raw);
    }
    let params = SetDeviceVolumeParams { device_index: Some(index), device_name: None, volume: cv };
    p.ack(if sink { Command::SetSinkVolume(params) } else { Command::SetSourceVolume(params) })
}

fn set_mute(p: &mut Pulse, sink: bool, index: u32, mute: bool) -> R<()> {
    let params = SetDeviceMuteParams { device_index: Some(index), device_name: None, mute };
    p.ack(if sink { Command::SetSinkMute(params) } else { Command::SetSourceMute(params) })
}

pub fn command(what: &str, args: &[SysValue]) -> R<()> {
    let mut p = Pulse::connect("pleamar")?;
    let (server, sinks, sources) = p.state()?;
    let sink = sinks.iter().find(|s| Some(&s.name) == server.default_sink_name.as_ref()).or(sinks.first());
    let source = sources.iter().filter(|s| s.monitor_of_sink_index.is_none()).find(|s| Some(&s.name) == server.default_source_name.as_ref());
    let out = |s: Option<&SinkInfo>| s.map(|s| (s.index, s.cvolume.channels().len(), level(&s.cvolume), s.muted)).ok_or("there is no output");
    let inp = |s: Option<&SourceInfo>| s.map(|s| (s.index, s.cvolume.channels().len(), level(&s.cvolume), s.muted)).ok_or("there is no input");
    match (what, args) {
        ("audio.volume", [SysValue::Num(v)]) => {
            let (i, ch, _, _) = out(sink)?;
            set_volume(&mut p, true, i, ch, *v)
        }
        // A step, as a fraction of one: 0.05 goes up, -0.05 goes down. Never above 100 %.
        ("audio.step", [SysValue::Num(d)]) => {
            let (i, ch, now, _) = out(sink)?;
            set_volume(&mut p, true, i, ch, now + d)
        }
        ("audio.mute", []) => {
            let (i, _, _, muted) = out(sink)?;
            set_mute(&mut p, true, i, !muted)
        }
        ("audio.mute", [SysValue::Bool(yes)]) => {
            let (i, _, _, _) = out(sink)?;
            set_mute(&mut p, true, i, *yes)
        }
        ("audio.input", [SysValue::Num(v)]) => {
            let (i, ch, _, _) = inp(source)?;
            set_volume(&mut p, false, i, ch, *v)
        }
        ("audio.input_mute", []) => {
            let (i, _, _, muted) = inp(source)?;
            set_mute(&mut p, false, i, !muted)
        }
        ("audio.input_mute", [SysValue::Bool(yes)]) => {
            let (i, _, _, _) = inp(source)?;
            set_mute(&mut p, false, i, *yes)
        }
        // Where it plays or listens: the number the lists give.
        ("audio.default", [SysValue::Num(id)]) => {
            let id = *id as u32;
            if let Some(s) = sinks.iter().find(|s| s.index == id) {
                return p.ack(Command::SetDefaultSink(s.name.clone()));
            }
            if let Some(s) = sources.iter().find(|s| s.index == id) {
                return p.ack(Command::SetDefaultSource(s.name.clone()));
            }
            Err(format!("there is no device {id}"))
        }
        _ => Err(format!("'{what}' is not asked like that: audio.volume(0..1), audio.step(±0.05), audio.mute([true|false]), audio.input(0..1), audio.input_mute([true|false]), audio.default(id)")),
    }
}
