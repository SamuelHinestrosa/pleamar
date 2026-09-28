//! Bluetooth, through BlueZ on the system bus: told of changes, not polled.
//!
//! `{ present, powered, discovering, devices = { { name, address, paired,
//!    connected, battery, icon }, … } }`: the first adapter, and the devices it
//! knows or sees, the connected first, then the paired. `battery` is 0..1, or
//! -1 when the device does not say. With no adapter, `{ present = false }`.
//!
//! Commands (`bluetooth.*`): `power(on)`, `scan(on)`, `connect(address)` —it
//! pairs first if it has to, and trusts it—, `disconnect(address)`,
//! `forget(address)`. Connecting takes seconds: it goes on its own thread, and
//! the change arrives through the service.

use super::SysValue;
use std::collections::HashMap;
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::MatchRule;

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn objects(c: &Connection) -> Option<Objects> {
    Proxy::new(c, BLUEZ, "/", "org.freedesktop.DBus.ObjectManager").ok()?.call("GetManagedObjects", &()).ok()
}

fn adapter(o: &Objects) -> Option<(&OwnedObjectPath, &HashMap<String, OwnedValue>)> {
    let mut all: Vec<_> = o.iter().filter_map(|(p, i)| i.get(ADAPTER).map(|a| (p, a))).collect();
    all.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    all.into_iter().next()
}

fn flag(m: &HashMap<String, OwnedValue>, k: &str) -> bool {
    m.get(k).and_then(|v| bool::try_from(v).ok()).unwrap_or(false)
}

fn word(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    m.get(k).and_then(|v| <&str>::try_from(v).ok()).unwrap_or_default().to_owned()
}

pub fn available() -> bool {
    Connection::system().ok().and_then(|c| objects(&c)).is_some_and(|o| adapter(&o).is_some())
}

fn now(c: &Connection) -> SysValue {
    let Some(o) = objects(c) else { return SysValue::Map(vec![("present".into(), SysValue::Bool(false))]) };
    let Some((adapter_path, a)) = adapter(&o) else { return SysValue::Map(vec![("present".into(), SysValue::Bool(false))]) };
    let mut devices: Vec<(bool, bool, SysValue)> = o
        .iter()
        .filter(|(p, _)| p.as_str().starts_with(adapter_path.as_str()))
        .filter_map(|(_, i)| {
            let d = i.get(DEVICE)?;
            let paired = flag(d, "Paired");
            let connected = flag(d, "Connected");
            // With no name of its own, BlueZ's alias is its address with dashes
            // («45-1B-85-DD-04-24»): that is no name.
            let address = word(d, "Address");
            let no_name = |n: &String| n.is_empty() || n.replace('-', ":").eq_ignore_ascii_case(&address);
            let name = [word(d, "Alias"), word(d, "Name")].into_iter().find(|n| !no_name(n)).unwrap_or_default();
            // A device that has not even said its name and was never paired is noise.
            if name.is_empty() && !paired {
                return None;
            }
            let name = if name.is_empty() { address.clone() } else { name };
            let battery = i.get(BATTERY).and_then(|b| b.get("Percentage")).and_then(|v| u8::try_from(v).ok()).map_or(-1.0, |p| p as f64 / 100.0);
            Some((
                connected,
                paired,
                SysValue::Map(vec![
                    ("name".into(), SysValue::Text(name)),
                    ("address".into(), SysValue::Text(address.clone())),
                    ("paired".into(), SysValue::Bool(paired)),
                    ("connected".into(), SysValue::Bool(connected)),
                    ("battery".into(), SysValue::Num(battery)),
                    ("icon".into(), SysValue::Text(word(d, "Icon"))),
                ]),
            ))
        })
        .collect();
    devices.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(format!("{:?}", a.2).cmp(&format!("{:?}", b.2))));
    devices.truncate(24);
    SysValue::Map(vec![
        ("present".into(), SysValue::Bool(true)),
        ("powered".into(), SysValue::Bool(flag(a, "Powered"))),
        ("discovering".into(), SysValue::Bool(flag(a, "Discovering"))),
        ("devices".into(), SysValue::List(devices.into_iter().map(|(_, _, v)| v).collect())),
    ])
}

pub fn service(dispatch: Box<dyn Fn(SysValue) + Send>) -> bool {
    let Ok(c) = Connection::system() else { return false };
    std::thread::Builder::new()
        .name("bluetooth".into())
        .spawn(move || {
            let mut last = String::new();
            let mut report = |c: &Connection| {
                let v = now(c);
                let fingerprint = format!("{v:?}");
                if fingerprint != last {
                    last = fingerprint;
                    dispatch(v);
                }
            };
            report(&c);
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            if let Ok(rule) = MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(BLUEZ).map(|b| b.build()) {
                let (c, tx) = (c.clone(), tx.clone());
                std::thread::spawn(move || {
                    let Ok(messages) = MessageIterator::for_match_rule(rule, &c, Some(64)) else { return };
                    for _ in messages {
                        if tx.send(()).is_err() {
                            break;
                        }
                    }
                });
            }
            loop {
                match rx.recv_timeout(Duration::from_secs(30)) {
                    Ok(()) => {
                        // Discovering is a stream of devices appearing: told in batches.
                        std::thread::sleep(Duration::from_millis(300));
                        while rx.try_recv().is_ok() {}
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
                report(&c);
            }
        })
        .is_ok()
}

fn device_path(o: &Objects, address: &str) -> Option<OwnedObjectPath> {
    o.iter().find(|(_, i)| i.get(DEVICE).is_some_and(|d| word(d, "Address").eq_ignore_ascii_case(address))).map(|(p, _)| p.clone())
}

/// An agent that says yes: what pairing a pair of headphones needs. Only while
/// pairing, and only for what this process asked for.
struct Agent;

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}
    fn request_pin_code(&self, _device: OwnedObjectPath) -> String {
        "0000".into()
    }
    fn display_pin_code(&self, _device: OwnedObjectPath, _pincode: String) {}
    fn request_passkey(&self, _device: OwnedObjectPath) -> u32 {
        0
    }
    fn display_passkey(&self, _device: OwnedObjectPath, _passkey: u32, _entered: u16) {}
    fn request_confirmation(&self, _device: OwnedObjectPath, _passkey: u32) {}
    fn request_authorization(&self, _device: OwnedObjectPath) {}
    fn authorize_service(&self, _device: OwnedObjectPath, _uuid: String) {}
    fn cancel(&self) {}
}

const AGENT_PATH: &str = "/org/pleamar/bluetooth_agent";

fn connect(address: String) -> Result<(), String> {
    let c = zbus::blocking::connection::Builder::system().and_then(|b| b.serve_at(AGENT_PATH, Agent)).and_then(|b| b.build()).map_err(|e| e.to_string())?;
    let o = objects(&c).ok_or("BlueZ does not answer")?;
    let path = device_path(&o, &address).ok_or("that device is not known: scan first")?;
    let device = Proxy::new(&c, BLUEZ, path.as_str(), DEVICE).map_err(|e| e.to_string())?;
    if !device.get_property::<bool>("Paired").unwrap_or(false) {
        let manager = Proxy::new(&c, BLUEZ, "/org/bluez", "org.bluez.AgentManager1").map_err(|e| e.to_string())?;
        let agent = OwnedObjectPath::try_from(AGENT_PATH).map_err(|e| e.to_string())?;
        let _ = manager.call_method("RegisterAgent", &(agent.clone(), "NoInputNoOutput"));
        let paired = device.call_method("Pair", &());
        let _ = manager.call_method("UnregisterAgent", &(agent,));
        paired.map_err(|e| format!("could not pair: {e}"))?;
        // Trusted: it may connect again by itself.
        let _ = device.set_property("Trusted", true);
    }
    device.call_method("Connect", &()).map(|_| ()).map_err(|e| format!("could not connect: {e}"))
}

/// The connection commands go through, kept: BlueZ stops a discovery the
/// moment whoever asked for it leaves the bus, and a connection per command
/// left as soon as it had asked —«scan» started and stopped at once—.
fn commands() -> Result<Connection, String> {
    static KEPT: std::sync::OnceLock<Connection> = std::sync::OnceLock::new();
    if let Some(c) = KEPT.get() {
        return Ok(c.clone());
    }
    let c = Connection::system().map_err(|e| e.to_string())?;
    Ok(KEPT.get_or_init(|| c).clone())
}

pub fn command(what: &str, args: &[SysValue]) -> Result<(), String> {
    let c = commands()?;
    let o = objects(&c).ok_or("BlueZ is not running")?;
    let (adapter_path, _) = adapter(&o).ok_or("there is no Bluetooth adapter")?;
    let adapter = Proxy::new(&c, BLUEZ, adapter_path.as_str(), ADAPTER).map_err(|e| e.to_string())?;
    let err = |e: zbus::Error| e.to_string();
    match (what, args) {
        ("bluetooth.power", [SysValue::Bool(on)]) => adapter.set_property("Powered", *on).map_err(|e| e.to_string()),
        ("bluetooth.scan", [SysValue::Bool(on)]) => adapter.call_method(if *on { "StartDiscovery" } else { "StopDiscovery" }, &()).map(|_| ()).map_err(err),
        ("bluetooth.connect", [SysValue::Text(address)]) => {
            let address = address.clone();
            // Pairing waits for the device: seconds, not for the logic to wait.
            std::thread::Builder::new()
                .name("bluetooth connect".into())
                .spawn(move || {
                    if let Err(e) = connect(address.clone()) {
                        eprintln!("bluetooth · {address}: {e}");
                    }
                })
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        ("bluetooth.disconnect", [SysValue::Text(address)]) => {
            let path = device_path(&o, address).ok_or("that device is not known")?;
            Proxy::new(&c, BLUEZ, path.as_str(), DEVICE).map_err(|e| e.to_string())?.call_method("Disconnect", &()).map(|_| ()).map_err(err)
        }
        ("bluetooth.forget", [SysValue::Text(address)]) => {
            let path = device_path(&o, address).ok_or("that device is not known")?;
            adapter.call_method("RemoveDevice", &(path,)).map(|_| ()).map_err(err)
        }
        _ => Err(format!("'{what}' is not asked like that: bluetooth.power(true|false), bluetooth.scan(true|false), bluetooth.connect(address), bluetooth.disconnect(address), bluetooth.forget(address)")),
    }
}
