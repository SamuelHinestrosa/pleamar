//! What the scene tells whoever drives it from outside —an agent, a script, a
//! screen reader one day—: what there is to read and touch, by name, with what
//! it says and where it is. Nothing here is guessed: it is the same data the
//! render draws with and presses with. See `docs/12-agents.md`.

use crate::gpu::{content_text, SeenText};
use crate::scene::{Ctx, Instr, Reach, Scene, Trigger, Zone};
use std::fmt::Write;

/// A piece of the scene that is on screen this frame: an open surface, or a
/// popup, and the stretch of the scene's plane it shows.
pub struct Shown {
    pub surface: usize,
    pub popup: Option<usize>,
    pub bounds: [f32; 4],
    pub scale: f32,
}

/// What the render knows this frame and the scene does not.
pub struct Sight<'a> {
    pub shown: Vec<Shown>,
    pub texts_seen: &'a [SeenText],
    /// By instruction: inside a group that is hidden, or in a copy whose
    /// monitor is not there. Its zones are not there either.
    pub gone: &'a dyn Fn(usize) -> bool,
    /// Which zone is on top of which, when groups with `z:` changed it.
    pub rank: Option<&'a [(usize, u8, usize)]>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role {
    Button,
    Item,
    Slider,
    Field,
    List,
    Region,
}

impl Role {
    fn word(self) -> &'static str {
        match self {
            Role::Button => "button",
            Role::Item => "item",
            Role::Slider => "slider",
            Role::Field => "field",
            Role::List => "list",
            Role::Region => "region",
        }
    }
}

/// One thing to read or touch.
#[derive(Debug)]
pub struct Node {
    pub zone: usize,
    pub name: &'static str,
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub inactive: bool,
    pub covered_by: Option<&'static str>,
    /// Scrolled out of its list: acting on it scrolls it into sight first.
    pub off_view: bool,
    pub persons: bool,
    /// In the surface's logical pixels: x, y, width, height.
    pub at: [f32; 4],
    /// The list it is inside, if any (an index into the nodes).
    pub inside: Option<usize>,
}

/// One surface or popup, with what it holds.
pub struct Part {
    pub name: String,
    pub kind: String,
    pub size: (f32, f32),
    pub scale: f32,
    pub nodes: Vec<Node>,
}

/// What a zone is, from what it does: its rules, its cursor, its field.
pub fn role_of(scene: &Scene, k: usize, z: &Zone) -> Role {
    if field_of(scene, z).is_some() {
        return Role::Field;
    }
    if z.scrolls.is_some() {
        return Role::List;
    }
    let mine = |t: &Trigger| match t {
        Trigger::Press(id) | Trigger::PressWith(id, _) | Trigger::Release(id) | Trigger::Receive(id) | Trigger::Carry(id) => (id.0 as usize == k, false),
        Trigger::Hold { zone, .. } => (zone.0 as usize == k, false),
        Trigger::Drag(id) | Trigger::Wheel(id) => (id.0 as usize == k, true),
        _ => (false, false),
    };
    let (mut presses, mut drags, mut wheels) = (false, false, false);
    for r in &scene.rules {
        match (mine(&r.when), &r.when) {
            ((true, true), Trigger::Drag(_)) => drags = true,
            ((true, true), _) => wheels = true,
            ((true, false), _) => presses = true,
            _ => {}
        }
    }
    // Dragged, or only turned with the wheel: a volume bar, a dial. A press
    // that also listens to the wheel is still a button (mute, and the wheel for the volume).
    if drags || (wheels && !presses) {
        return Role::Slider;
    }
    if presses || z.cursor == crate::scene::Cursor::Hand || z.carries.is_some() {
        // A copy of a `for` or a `repeat` (`hit#r3`), not of a component (`hit#Row5`):
        // one of a list's rows.
        let copy = z.id.rsplit('#').next().filter(|_| z.id.contains('#'));
        if copy.is_some_and(|s| s.starts_with(|c: char| c.is_ascii_lowercase()) && !s.starts_with("screen") && !s.starts_with("between")) {
            return Role::Item;
        }
        return Role::Button;
    }
    Role::Region
}

/// The field a zone belongs to: its text, its placeholder and whether it is secret.
fn field_of<'s>(scene: &'s Scene, z: &Zone) -> Option<(usize, &'s crate::scene::Content, bool)> {
    match scene.instrs.get(z.at) {
        Some(Instr::Field { text, zone, placeholder, secret, .. }) if *zone == z.id => Some((text.0 as usize, placeholder, *secret)),
        _ => scene.instrs.iter().find_map(|i| match i {
            Instr::Field { text, zone, placeholder, secret, .. } if *zone == z.id => Some((text.0 as usize, placeholder, *secret)),
            _ => None,
        }),
    }
}

fn area(b: [f32; 4]) -> f32 {
    (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)
}

fn inside(b: [f32; 4], x: f32, y: f32) -> bool {
    x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3]
}

/// The scene as it is this frame: every surface and popup on screen, and in
/// each one what can be read and touched.
pub fn describe(scene: &Scene, c: Ctx, texts: &[String], sight: &Sight) -> Vec<Part> {
    // Which zones are there at all, and their box.
    let there: Vec<Option<[f32; 4]>> = scene
        .zones
        .iter()
        .map(|z| if z.reach == Reach::Hidden || (sight.gone)(z.at) { None } else { z.bounds(c) })
        .collect();
    let active: Vec<bool> = scene.zones.iter().map(|z| z.active.is_true(c)).collect();
    let rank = |k: usize| sight.rank.map_or((k, 0, 0), |r| r[k]);
    // The one on top at a point: what a press there would reach.
    let top_at = |x: f32, y: f32| {
        scene
            .zones
            .iter()
            .enumerate()
            .filter(|(k, z)| there[*k].is_some() && z.active.is_true(c) && z.contains(c, x, y))
            .max_by_key(|(k, _)| (rank(*k), *k))
            .map(|(k, _)| k)
    };
    let mut parts = Vec::new();
    for s in &sight.shown {
        let Some(surface) = scene.surfaces.get(s.surface) else { continue };
        // The lock screen is never told, nor what is kept out of captures or from agents.
        if surface.lock_screen || surface.hidden_from_captures || surface.agent_hidden {
            continue;
        }
        let (name, kind) = match s.popup.and_then(|p| scene.popups.get(p)) {
            Some(p) => (p.name.to_owned(), "popup".to_owned()),
            None => {
                let name = if surface.name.is_empty() { "main".to_owned() } else { surface.name.clone() };
                let name = if surface.instance > 0 || matches!(surface.screens, crate::scene::Screens::Number(_)) { format!("{name} (screen {})", surface.instance) } else { name };
                let kind = match &surface.window {
                    Some(t) if !t.is_empty() => format!("window «{t}»"),
                    Some(_) => "window".to_owned(),
                    None => "panel".to_owned(),
                };
                (name, kind)
            }
        };
        let b = s.bounds;
        let centre = |z: [f32; 4]| ((z[0] + z[2]) * 0.5, (z[1] + z[3]) * 0.5);
        let on_this = |k: usize| there[k].is_some_and(|z| {
            let (x, y) = centre(z);
            inside(b, x, y)
        });
        // Scrolled out of its list: its centre is not inside the list's window.
        let off_view = |k: usize| {
            scene.zones[k].within.is_some_and(|l| match (there[l.0 as usize], there[k]) {
                (Some(w), Some(z)) => {
                    let (x, y) = centre(z);
                    !inside(w, x, y)
                }
                _ => false,
            })
        };
        // What is on this surface, and the rows of its lists that are scrolled out of it.
        let mine: Vec<usize> = (0..scene.zones.len())
            .filter(|k| there[*k].is_some() && (on_this(*k) || scene.zones[*k].within.is_some_and(|l| on_this(l.0 as usize))))
            .collect();
        // Each text goes to the smallest zone it falls in: a button's word is
        // the button's, not the panel's around it. A text cut out by a clip
        // only says something for a row scrolled out of sight.
        let mut said: Vec<Vec<&SeenText>> = vec![Vec::new(); scene.zones.len()];
        for t in sight.texts_seen {
            let (x, y) = centre(t.bounds);
            // An active zone first: a closed menu's zone, still in its place, does not
            // take the words of what is drawn where it would be.
            let smallest = mine
                .iter()
                .filter(|k| there[**k].is_some_and(|z| inside(z, x, y)) && scene.zones[**k].scrolls.is_none() && (!t.clipped || off_view(**k)))
                .min_by(|p, q| (!active[**p], area(there[**p].unwrap())).partial_cmp(&(!active[**q], area(there[**q].unwrap()))).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(k) = smallest {
                said[*k].push(t);
            }
        }
        let mut nodes: Vec<Node> = Vec::new();
        // Inside a copy per monitor every name ends in `#screen0`: said once, in the header.
        let suffix = format!("#screen{}", surface.instance);
        for &k in &mine {
            let z = &scene.zones[k];
            let zb = there[k].unwrap();
            let active = active[k];
            let role = role_of(scene, k, z);
            let mut words: Vec<&SeenText> = said[k].clone();
            // In reading order: by line, then from the left.
            words.sort_by(|p, q| ((p.bounds[1] / 8.0).round(), p.bounds[0]).partial_cmp(&((q.bounds[1] / 8.0).round(), q.bounds[0])).unwrap_or(std::cmp::Ordering::Equal));
            // An inactive zone with nothing drawn in it is not there for anyone: a
            // closed panel's buttons. One with its words is a button seen greyed out.
            if !active && words.is_empty() {
                continue;
            }
            let field = field_of(scene, z);
            let label = match (&z.label, field) {
                (Some(l), _) => content_text(l, c, texts).into_owned(),
                (None, Some((text, placeholder, _))) if texts.get(text).is_none_or(|t| t.is_empty()) => content_text(placeholder, c, texts).into_owned(),
                (None, Some(_)) => String::new(),
                (None, None) => words.iter().map(|t| t.text.trim()).collect::<Vec<_>>().join(" "),
            };
            let value = field.map(|(text, _, secret)| if secret || z.reach == Reach::Person { "(hidden)".to_owned() } else { texts.get(text).cloned().unwrap_or_default() });
            let (cx, cy) = ((zb[0] + zb[2]) * 0.5, (zb[1] + zb[3]) * 0.5);
            let off = off_view(k);
            let covered_by = match (active && !off, role) {
                (true, Role::Button | Role::Item | Role::Field | Role::Slider) => top_at(cx, cy).filter(|t| *t != k && scene.zones[*t].scrolls.is_none()).map(|t| scene.zones[t].id.strip_suffix(suffix.as_str()).unwrap_or(scene.zones[t].id)),
                _ => None,
            };
            nodes.push(Node {
                zone: k,
                name: z.id.strip_suffix(suffix.as_str()).unwrap_or(z.id),
                role,
                label,
                value,
                inactive: !active,
                covered_by,
                off_view: off,
                persons: z.reach == Reach::Person,
                at: [zb[0] - b[0], zb[1] - b[1], zb[2] - zb[0], zb[3] - zb[1]],
                inside: None,
            });
        }
        // What is inside a list hangs from it: the rows it scrolls, and
        // whatever else falls inside its window.
        for i in 0..nodes.len() {
            if let Some(j) = scene.zones[nodes[i].zone].within.and_then(|l| nodes.iter().position(|n| n.zone == l.0 as usize)) {
                nodes[i].inside = Some(j);
                continue;
            }
            let (x, y) = (nodes[i].at[0] + nodes[i].at[2] * 0.5, nodes[i].at[1] + nodes[i].at[3] * 0.5);
            nodes[i].inside = (0..nodes.len())
                .filter(|j| *j != i && nodes[*j].role == Role::List)
                .filter(|j| {
                    let a = nodes[*j].at;
                    inside([a[0], a[1], a[0] + a[2], a[1] + a[3]], x, y)
                })
                .min_by(|p, q| (nodes[*p].at[2] * nodes[*p].at[3]).total_cmp(&(nodes[*q].at[2] * nodes[*q].at[3])));
        }
        parts.push(Part { name, kind, size: (b[2] - b[0], b[3] - b[1]), scale: s.scale, nodes });
    }
    parts
}

/// The zones a person could press but that nobody says what they are: what
/// a screen reader would read as «button», and nothing else.
pub fn unnamed(parts: &[Part]) -> Vec<&'static str> {
    parts.iter().flat_map(|p| &p.nodes).filter(|n| matches!(n.role, Role::Button | Role::Item | Role::Slider) && n.label.is_empty()).map(|n| n.name).collect()
}

fn round(v: f32) -> i32 {
    v.round() as i32
}

/// As text: one line per thing, indented under the list that holds it.
pub fn to_text(parts: &[Part]) -> String {
    let mut out = String::new();
    if parts.is_empty() {
        out.push_str("nothing on screen\n");
    }
    for p in parts {
        let _ = writeln!(out, "{} · {} {}×{} · scale {}", p.name, p.kind, round(p.size.0), round(p.size.1), p.scale);
        let width = p.nodes.iter().map(|n| n.name.chars().count()).max().unwrap_or(0);
        let mut write = |n: &Node, depth: usize| {
            let mut line = format!("{}{:<7} {:<width$}", "  ".repeat(depth + 1), n.role.word(), n.name);
            if !n.label.is_empty() {
                let _ = write!(line, "  «{}»", n.label);
            }
            if let Some(v) = &n.value {
                let _ = write!(line, "  \"{v}\"");
            }
            if n.inactive {
                line.push_str("  · inactive");
            }
            if let Some(t) = n.covered_by {
                let _ = write!(line, "  · covered by {t}");
            }
            if n.off_view {
                line.push_str("  · off view");
            }
            if n.persons {
                line.push_str("  · a person's");
            }
            let _ = writeln!(out, "{line}  at {},{} {}×{}", round(n.at[0]), round(n.at[1]), round(n.at[2]), round(n.at[3]));
        };
        // Depth first, in the order they were written.
        fn walk(nodes: &[Node], parent: Option<usize>, depth: usize, write: &mut dyn FnMut(&Node, usize)) {
            for (i, n) in nodes.iter().enumerate() {
                if n.inside == parent {
                    write(n, depth);
                    walk(nodes, Some(i), depth + 1, write);
                }
            }
        }
        walk(&p.nodes, None, 0, &mut write);
    }
    out
}

/// As JSON, for a program.
pub fn to_json(parts: &[Part]) -> String {
    fn node(nodes: &[Node], i: usize) -> serde_json::Value {
        let n = &nodes[i];
        let mut v = serde_json::json!({
            "name": n.name,
            "role": n.role.word(),
            "label": n.label,
            "box": [round(n.at[0]), round(n.at[1]), round(n.at[2]), round(n.at[3])],
            "active": !n.inactive,
            "agent": if n.persons { "no" } else { "yes" },
        });
        if let Some(t) = &n.value {
            v["value"] = t.clone().into();
        }
        if let Some(t) = n.covered_by {
            v["covered_by"] = t.into();
        }
        if n.off_view {
            v["off_view"] = true.into();
        }
        let inner: Vec<serde_json::Value> = (0..nodes.len()).filter(|j| nodes[*j].inside == Some(i)).map(|j| node(nodes, j)).collect();
        if !inner.is_empty() {
            v["children"] = inner.into();
        }
        v
    }
    let all: Vec<serde_json::Value> = parts
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "kind": p.kind,
                "size": [round(p.size.0), round(p.size.1)],
                "scale": p.scale,
                "nodes": (0..p.nodes.len()).filter(|i| p.nodes[*i].inside.is_none()).map(|i| node(&p.nodes, i)).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::to_string(&all).unwrap_or_default() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Animated;

    /// `tests/agent.plm` as it starts, with every surface on screen.
    fn told(dirty: bool) -> Vec<Part> {
        let (scene, _) = crate::language::read_file(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/agent.plm")).unwrap();
        let props: Vec<Animated> = scene.props.iter().map(|(_, v, s)| Animated { x: *v, v: 0.0, target: *v, spring: *s }).collect();
        let mut facts: Vec<f32> = scene.facts.iter().map(|f| f.1).collect();
        if dirty {
            facts[scene.facts.iter().position(|f| f.0 == "dirty").unwrap()] = 1.0;
        }
        let texts: Vec<String> = scene.texts.iter().map(|t| t.1.clone()).collect();
        let shown = scene
            .surfaces
            .iter()
            .enumerate()
            .map(|(k, s)| Shown { surface: k, popup: None, bounds: [s.origin.0, s.origin.1, s.origin.0 + s.width as f32, s.origin.1 + s.height as f32], scale: 1.0 })
            .collect();
        let never = |_: usize| false;
        let sight = Sight { shown, texts_seen: &[], gone: &never, rank: None };
        describe(&scene, Ctx { props: &props, facts: &facts }, &texts, &sight)
    }

    fn node<'p>(parts: &'p [Part], name: &str) -> Option<&'p Node> {
        parts.iter().flat_map(|p| &p.nodes).find(|n| n.name == name)
    }

    #[test]
    fn a_scene_says_what_it_holds() {
        let parts = told(false);
        // The vault is `agent: hidden`: only the window is told.
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, "window «Told»");
        let query = node(&parts, "query").unwrap();
        assert_eq!((query.role, query.label.as_str(), query.value.as_deref()), (Role::Field, "Search notes", Some("")));
        // A secret field, and one kept for a person: its value is not given.
        let pin = node(&parts, "pin").unwrap();
        assert_eq!((pin.value.as_deref(), pin.persons), (Some("(hidden)"), true));
        // Dragged: sliders, each copy with its own word.
        let knobs: Vec<(Role, &str)> = (0..3).map(|k| node(&parts, &format!("knob.{k}")).map(|n| (n.role, n.label.as_str())).unwrap()).collect();
        assert_eq!(knobs, [(Role::Slider, "Brightness"), (Role::Slider, "Volume"), (Role::Slider, "Microphone")]);
        let delete = node(&parts, "delete").unwrap();
        assert_eq!((delete.role, delete.persons), (Role::Button, true));
        // Hidden is not even named; inactive with nothing drawn is not there.
        assert!(node(&parts, "private").is_none());
        assert!(node(&parts, "save").is_none());
        // And what can be pressed with nothing saying what it is gets pointed at.
        assert_eq!(unnamed(&parts), ["close"]);
        let text = to_text(&parts);
        assert!(text.contains("«Delete»  · a person's"), "{text}");
    }

    #[test]
    fn a_label_follows_what_it_names() {
        let parts = told(true);
        let save = node(&parts, "save").unwrap();
        assert_eq!((save.label.as_str(), save.inactive), ("Save Ready", false));
        let json: serde_json::Value = serde_json::from_str(&to_json(&parts)).unwrap();
        assert!(json[0]["nodes"].as_array().unwrap().iter().any(|n| n["name"] == "save" && n["label"] == "Save Ready"));
    }
}
