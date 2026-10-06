# Scenes an agent can read — design

**Status:** step 1 (`describe`, `label:`, `agent:`) implemented on 6 Oct 2026; the rest is design. Agreed with Abel on 6 Oct 2026 as the first step towards pleamar as a way to give **any program** its interface. This note is the first stretch of that road: what the scene already knows, handed to whoever drives it from outside.

## 1. Why

An agent that uses a desktop today works with pixels. `pleamar-wm agent look PID` gives it a picture, it guesses where the button is, `click PID 412 38` presses there, and then it looks again to find out what happened. The help of `pleamar-wm agent` ends with "look before each click: a page moves under you". It is slow, it spends most of its tokens on pictures, and it breaks when something moves.

Accessibility trees (AT-SPI, UIA) were meant to fix that, and they do so half-way: in GTK, Qt or a browser the tree is whatever each widget remembered to say, so agents end up falling back on the picture.

**A pleamar scene does not need to be guessed.** The compiler knows every zone with its name, every field, every text, every event and which rule fires it. The render knows, every frame, where each one is, whether it is active and whether something covers it. Nothing has to be added to a scene to get a tree: it is complete by construction, because it is the same data that draws it.

## 2. What it adds

Four things, all on the socket `--say` already uses:

1. **`describe`**: the scene as a tree of what can be read and touched.
2. **Acting by name**: `press save`, `type search factura`, through the same path a hand takes.
3. **`wait`** for a condition, and **`watch`** what changes: no more taking pictures in a loop to see whether it has finished.
4. **`agent: no`**: what the scene keeps for a person's hand.

And three small properties for when what is derived is not enough: `label:`, `role:`, `value:`.

## 3. The tree

Implemented (step 1, 6 Oct 2026). This is what it answers for a small notes window, with three rows in sight and two scrolled out of it:

```
$ pleamar --say notes describe
main · window «Notes» 640×480 · scale 1
  field   query   «Search notes»  ""  at 16,9 240×27
  field   pin     «PIN»  "(hidden)"  at 300,9 120×27
  list    list    at 16,60 608×120
    item    hit#r0  «October invoice»  at 16,60 608×38
    item    hit#r1  «Shopping list»  at 16,100 608×38
    item    hit#r2  «pleamar ideas»  at 16,140 608×38
    item    hit#r3  «Fourth note»  · off view  at 16,180 608×38
    item    hit#r4  «Fifth note»  · off view  at 16,220 608×38
  text            «No note open»  at 16,390 96×20
  button  new     «New note»  at 452,428 80×36
  button  save    «Save»  · inactive  at 544,428 80×36
  button  delete  «Delete»  · a person's  at 360,428 80×36
  button  close   at 600,14 20×20
```

`describe json` gives the same as data, with the rows of a list in its `children`. Asking costs one frame: the answer is made after the next list is composed, when the render knows where every text went (17 ms for Marea's 1169 zones).

| Part | Where it comes from |
| --- | --- |
| **header** | One per surface or popup on screen: its name (`main` for the scene's own), what it is (`window «title»`, `panel`, `popup`), its size and its scale. A copy per monitor says `(screen 0)` there, and its names lose the `#screen0` every one of them carries |
| **name** | The zone's or the field's own, as the scene knows it: `knob.2` for one written `knob.$k`, `hit#r3` for the `hit` of the fourth copy of a `for r`, `touch#TrayIcon431` for one inside a copy of a component. It is the name `press` will take (step 2) |
| **role** | Derived: an `input` is a `field`; a stack with `view:` a `list`; a zone with `on drag`, or with `on wheel` and no press, a `slider`; one with a press, `cursor: pointer` or `carries:`, a `button`, or an `item` if it is in a copy of a `for` or a `repeat`; anything else a `region`. And the words drawn outside every zone —a title, a status line— are a `text`, with no name: they are read, not touched. `role:` will override it (step 7) |
| **label** | `label:` if it has one. If not, the texts drawn inside its box this frame, in reading order: each text goes to the smallest **active** zone it falls in, so a button's word is the button's and not the panel's around it, and a closed menu still in its place does not take the words of what is drawn there. A field without text says its `placeholder`. **Nothing is guessed from what is near**: in Marea the slider's name is drawn under its icon, not under the slider, and a guess would have named the icon. When the word is not inside, `label:` says it |
| **value** | A field's text: `(hidden)` with `secret: true` or `agent: no`. `value:` for the rest will come with step 7 |
| **state** | `inactive` when its `active:` is false **and** something is drawn in it (a greyed out button; an inactive zone with nothing in it is a closed panel's, and is left out), `covered by X` when another zone is on top at its centre, `off view` when its list has scrolled it out, `a person's` with `agent: no` |
| **box** | In the surface's logical pixels, with the scale in the header: `pleamar-wm` turns it into the pixels of its `look` |
| **nesting** | The rows of a list hang from it, the ones in sight and the ones scrolled out, and so does whatever else falls in its window. Groups and components add no level: they are drawing, not meaning |

**What is not described.** A closed surface; a zone that is not there (`show:` false, or inside a hidden group); a surface with `captures: hidden`, because what is kept out of a screenshot is kept out of this too; anything with `agent: hidden` (section 10); and **never a `kind: lock`**: an agent does not get to read or touch the lock screen.

**A zone that says nothing is pointed at.** The first time the scene is asked, each zone that can be pressed and has neither `label:` nor a word inside it is named in the log, once, by the name it was written with: `agent  · 'close' can be pressed, but nothing says what it is: give it a \`label:\``. It is in the log and not in the compiler because whether a word falls inside is only known when it is drawn.

## 4. Acting by name

| Command | What it does | It answers |
| --- | --- | --- |
| `press NAME [right\|middle] [N]` | A press and release at the centre of the zone, N times | What happened |
| `hold NAME` | Press, wait for its `hold`, release | What happened |
| `drag NAME DX DY` | Press at its centre, move by DX, DY over half a second, release | What happened |
| `wheel NAME N` | N notches over it (positive, upwards) | What happened |
| `type FIELD TEXT` | Focus the field and leave TEXT in it, as typing would | What happened |
| `submit FIELD [TEXT]` | The same, then Enter (it exists already) | What happened |
| `key NAME` | A key to the surface with the keyboard: `escape`, `ctrl+z` | What happened |

**It goes through the same path as a hand**, not round it. `press save` is a pointer that enters the zone, a button down and a button up, fed into the render as `--mouse` already does: the zone lights up, its gesture plays, `pointer.x` and `local.x` have their values, its rules fire in their order, and the logic hears `press:save` as it always does. That is the point: a scene does not have to be written twice, and what an agent does is what a person would have done. `emit` and `fact` stay for scripts that want to skip all that.

**It refuses what a person could not do either**, and says why: `? save is inactive`, `? save is covered by dialog.backdrop`, `? delete is for a person's hand (agent: no)`. A press that is refused does not happen.

**What happened** is the answer to every action, read during the half second after it (or until nothing moves): the events that fired, the facts and texts that changed, and the surfaces that opened or closed.

```
$ pleamar --say notes "press new"
pressed new
  event  created
  fact   editing: false → true
  text   title: "" → "Untitled"
  opened surface editor
```

Most of the time the agent does not need to look again.

## 5. Waiting and watching

`wait EXPR [TIMEOUT]` answers as soon as the expression holds, or says it did not after TIMEOUT (5 s if unsaid). EXPR is an ordinary pleamar expression, checked with the scene's names, so a typo gets the usual "did you mean…?" instead of a wait that never ends:

```
$ pleamar --say notes "wait saving == false and rows.count > 0"
yes, after 640 ms
$ pleamar --say notes "wait sync == done 2s"
? not after 2 s: sync is failed
```

`watch` keeps the socket open and writes one line per change, the same lines as "what happened", until it is closed. It replaces the loop of looking at the picture, and costs nothing when nothing happens.

## 6. `agent: no`: for a person's hand

```
box delete { from: …; size: …; agent: no }
box pay    { from: …; size: …; agent: no; label: "Pay {total} €" }
```

A zone with `agent: no` is described (an agent knows it is there and can tell the person to press it) but cannot be pressed through the socket. On a field, its value is not given either. **No toolkit has this today**, and it is what lets someone hand a program to an agent while keeping the final word over paying, deleting or sending.

**What it is and what it is not.** Through the socket it is a promise the commands keep. It is **not** a wall against a program that runs as the same user: that one could fake input in many ways. What makes it a wall is the compositor:

- **pleamar-wm** gives the agent **its own seat** (`cua-agent`). pleamar learns which seat each press comes from (`wl_seat.name`), and **a press on an `agent: no` zone from an agent's seat is dropped**, and said in `watch`. So clicking its pixels with `pleamar-wm agent click` does not get round it. For that, pleamar-wm has to send its input to pleamar windows through the agent's seat, never through the usual one (today it does that for programs that only hear one seat).
- **Other compositors** have no agent seat: there it is only the promise.

## 7. In pleamar-wm

- pleamar answers `hello` with its PID, its scene and its language version. With that, `pleamar-wm agent windows` marks which windows speak pleamar.
- New commands that pass straight through: `agent tree PID`, `agent press PID NAME`, `agent type PID FIELD TEXT`, `agent wait PID EXPR`, `agent watch PID`. Every other window keeps `look` and `click`.
- **pleamar-wm's own shell is a pleamar scene**: its dock, its top bar and the overview become readable and touchable by name with this, for free.

## 8. What comes after, and why the tree is shaped like this

- **Screen readers.** The same tree, handed to AccessKit, becomes AT-SPI on Linux, UIA on Windows and NSAccessibility on macOS. That is why the roles are a subset of AccessKit's (`button`, `slider`, `field`, `list`, `item`, `region`; later `toggle`, `tab`, `link`), and why `label:` is a text that translates.
- **MCP.** `pleamar mcp SCENE` would offer `describe`, the actions, `wait` and `watch` as tools, and each event with `->` as a tool of its own. Every pleamar program would be an MCP server with no work.
- **A program in any language** driving a scene over this same socket (step 3 of the road): the commands above are its first half.
- **`checked:` and `selected:`** on zones, for toggles and lists that say which row is chosen. Left out of step 1: they are new state, the rest is reading what exists.

## 9. Order of work

| # | Piece | Checked by |
| --- | --- | --- |
| 1 ✅ | `describe` with derived roles and labels, `label:`, `agent:`, the warning | `tests/agent*.plm` and `label-not-a-text.plm` in `run-tests.sh`; `src/agent.rs`'s tests build the tree of `tests/agent.plm`; by hand, a notes window and Marea in a headless pleamar-wm |
| 2 | `press`, `hold`, `drag`, `wheel`, `type`, `key`, with their refusals and "what happened" | The same tests, plus Marea in the test copy: open the control centre by name |
| 3 | `wait` and `watch` | A test that waits for a fact a rule sets after a timer |
| 4 | `agent: no` and `agent: hidden` over the socket (they already shape `describe`) | A test that is refused |
| 5 | `hello`, and pleamar-wm's `agent tree/press/type/wait/watch` | Headless pleamar-wm with Marea |
| 6 | The seat: pleamar reads `wl_seat.name`, pleamar-wm uses the agent seat for pleamar windows, the press is dropped | Headless pleamar-wm: `agent click` on an `agent: no` zone does nothing |
| 7 | `role:`, `value:` | Tests |

Steps 1 to 4 are pleamar alone and work on any compositor. 5 and 6 need both.

## 10. Decided

- **`agent: hidden`** exists: a zone, a field or a surface with it is not described at all, and cannot be acted on. For what is private but not secret, like the text of a notification. `agent: no` is seen and not touched; `agent: hidden` is not even seen.
- **Off view, it scrolls by itself**, as a person would: `press row.40` moves its `view:` with its own spring until the row is in sight, and then presses. Whoever is watching sees the list glide to it, not jump. The answer says so: `scrolled rows to row.40`.
- **Who may ask: the socket's owner.** It is the user's Unix socket in the runtime directory, as `--say` is today. Nothing more is asked for.
