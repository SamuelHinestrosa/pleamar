---
name: pleamar-desktop
description: Use the user's desktop in a pleamar-wm session — see the windows, click, type, scroll and drag in any program (a browser, an editor, a chat) with the agent's own cursor and keyboard, while the user's mouse and keyboard stay theirs. Use when asked to do something in an app that is on screen ("go to reddit", "open X in the browser", "fill this form", "save that post", "look at what is on my screen"), and the session is pleamar-wm (`pleamar-wm agent windows` answers).
---
<!-- pleamar skill {VERSION}. Written by `pleamar --install-skill` and rewritten when pleamar updates: put your own notes in another skill. -->

# Using the desktop through pleamar-wm

pleamar-wm gives an agent hands of its own: a pointer and a keyboard on a seat
apart from the user's. What you do lands in the window you name even if it is
under another one or does not have the keyboard, and the user keeps working
meanwhile. They see you: your cursor is drawn (mint, with «agent» beside it),
the monitor you work on glows at its edges, and the window you touch gets an
outline.

It needs `agent on` in `~/.config/pleamar/session.conf` (and a session started
after that line was there). If `pleamar-wm agent windows` says there is no
agent socket, tell the user that line is what is missing; do not add it
yourself unless they ask.

## The loop: look, act, look

```sh
pleamar-wm agent windows                 # every window: its name (PID, or PID.N for one of a program's several), program, title, box, monitor, keyboard, and «pleamar scene» for one that can be asked
pleamar-wm agent open firefox            # start a program for your work: see below
pleamar-wm agent look PID [FILE]         # a picture of that window; prints FILE WxH
pleamar-wm agent click PID X Y           # X, Y are pixels of that picture
pleamar-wm agent type PID "some text"
pleamar-wm agent key PID enter           # tab escape backspace space up down left right delete home end pageup pagedown f1…f12
pleamar-wm agent hotkey PID ctrl+l
pleamar-wm agent scroll PID X Y down 5   # up down left right, and how many steps
pleamar-wm agent drag PID X1 Y1 X2 Y2
pleamar-wm agent click PID X Y right     # left right middle, and a count: … left 2 for a double click
pleamar-wm agent focus PID               # give it the user's keyboard and show its workspace (rarely needed)
pleamar-wm agent monitors                # the monitors, numbered
pleamar-wm agent send PID 1              # that window to monitor 1 (when the user names a monitor)
pleamar-wm agent done                    # finished: the light on the user's monitor goes out
pleamar-wm agent help
```

0. **A program that is not open**: `open COMMAND` (`open --monitor 1 …` when
   the user names one). The monitor you will work on lights up first, so the
   user sees it coming, and the window opens there —one the user is not on—
   without taking their keyboard. Never start programs from your shell (`&`,
   `setsid`, `xdg-open`): they open wherever the user's pointer is and take
   their keyboard.
1. **Find the window**: `windows`, and take its name: its PID, or `PID.N`
   when the program has several windows (a browser's windows belong to its
   main process).
2. **Look** at it: `look PID`, then read the picture. A window that is not
   seen (another workspace, put away) cannot be looked at: `focus PID` first,
   and say so to the user, since it moves what they see.
3. **Act** with the coordinates of that picture: what you see at (x, y) is
   where `click PID x y` lands.
4. **Look again** after anything that changes the page. Pages move: a banner,
   a notice or a dialog appears and what was at (x, y) is something else. A
   click on the wrong thing is worse than one more look.

   A **menu** (a right click, a dropdown) opens over the window: the picture
   shows it, and you click its item like anything else. A **dialog** (save
   as, open, «replace it?») is a window of its own, often of another program
   (the file chooser is a portal's): while it is open, `look` and the actions
   on the program's name reach the dialog, and `windows` lists it as
   «dialog of PID».
5. **Say you are done**: `pleamar-wm agent done` when the task is over. While
   you work, the user's monitor glows; without it the glow waits a minute and
   a half in case you are only thinking.

## A pleamar window: ask it, and act by name

`windows` marks a window made with pleamar with «pleamar scene NAME». Such a
window is asked what is on it and used by name, with no picture and no
coordinates:

```sh
pleamar-wm agent tree PID                       # every button, slider, field, list, item and text: name, what it says, state, box
pleamar-wm agent press PID save                 # your cursor glides to it and it is pressed; also: press PID X right 2
pleamar-wm agent wait PID 'status == "Saved" 5s'  # answers as soon as it holds: no looking again and again
pleamar-wm agent watch PID 10                   # a line for each thing that happens on it, for 10 s
pleamar-wm agent say PID "type query some words"  # any other order: drag knob.1 0 -40, hold X, wheel X 3, key escape
```

(Outside pleamar-wm, `pleamar --say NAME describe`, `"press save"`, `wait …`
reach the scene by its name, in `$PLEAMAR_SOCKETS`.)

Each line of `tree` is a thing's name, what it is, what it says («Save»), its
state (`inactive`, `covered by X`, `off view`, `a person's`) and its box.
**Each action answers with what happened** —events, facts and texts that
changed, lists that scrolled, surfaces that opened—, so you do not need to look
again; a row `off view` is scrolled into sight by itself. For what takes time
—saving, loading, a search— `wait` on what the window will say or hold when it
is done, instead of looking until it shows. What cannot be done is refused with
the reason (`? save is inactive`). **`a person's` means the scene keeps it for
the user's hand** (`agent: no`): it is refused, ask them to press it. Prefer
this to `look` and `click` for a pleamar window: it is several times faster and
cannot miss.

## What works best

- **The keyboard before the mouse**, where there is a shortcut: in a browser
  `hotkey PID ctrl+l`, then `type` the address and `key enter` goes anywhere
  without hunting for the address bar. `ctrl+t`, `ctrl+w`, `ctrl+f` too.
- **Click a field before typing in it**, and look to check the caret is there.
- **Any text can be typed**: accents, ñ and emoji too.
- **Scroll with the pointer over what scrolls**: the page, not a sidebar.
- Things that open on their own (a translation offer, a cookie banner, a
  video that plays) are the page's, not yours: close them if they are in the
  way, and mention them.

## What is the user's to decide

You are acting as the user, in their accounts. **Before anything that
publishes, sends, buys, deletes, follows, likes, accepts terms or changes a
setting, stop and ask**, even if it looks like part of the task. Drafts,
searches, reading, opening and saving for later are fine. Never type a
password or a payment detail; leave that to them. If a page asks for a login
or a captcha, stop there and say so.

The user can stop you at any moment (the «Stop» on the monitor's pill, or
`pleamar-wm agent stop`): then whatever you try answers that **the user
stopped the agent**. Stop there, do not try again, and say where you left it.

When you finish, say what you did, step by step, and what you left for them.

## Other agents' tools

The same hands speak `cua-inject v1`, the protocol of
[Cua Driver](https://github.com/trycua/cua): programs started in the session
get `CUA_INJECT_SOCKET` and `CUA_DRIVER_RS_ENABLE_WAYLAND=1`, so `cua-driver`
types, presses keys and uses accessibility actions there as it is. Its clicks
by coordinates need its captures to be trusted, which pleamar-wm answers with
an extra command (`r PID`) that the released Cua Driver does not ask yet;
`pleamar-wm agent` does everything meanwhile. `pleamar-wm agent raw LINE…`
speaks the protocol directly.
