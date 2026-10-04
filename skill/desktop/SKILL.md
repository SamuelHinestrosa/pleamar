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
pleamar-wm agent windows                 # every window: PID, program, title, box, seen, keyboard
pleamar-wm agent look PID [FILE]         # a picture of that window; prints FILE WxH
pleamar-wm agent click PID X Y           # X, Y are pixels of that picture
pleamar-wm agent type PID "some text"
pleamar-wm agent key PID enter           # tab escape backspace space up down left right delete home end pageup pagedown f1…f12
pleamar-wm agent hotkey PID ctrl+l
pleamar-wm agent scroll PID X Y down 5   # up down left right, and how many steps
pleamar-wm agent drag PID X1 Y1 X2 Y2
pleamar-wm agent click PID X Y right     # left right middle, and a count: … left 2 for a double click
pleamar-wm agent focus PID               # give it the user's keyboard and show its workspace (rarely needed)
pleamar-wm agent done                    # finished: the light on the user's monitor goes out
pleamar-wm agent help
```

1. **Find the window**: `windows`, and take its PID (a browser's window
   belongs to its main process; `pgrep -o firefox`, `pgrep -f …/zen$` also
   find it).
2. **Look** at it: `look PID`, then read the picture. A window that is not
   seen (another workspace, put away) cannot be looked at: `focus PID` first,
   and say so to the user, since it moves what they see.
3. **Act** with the coordinates of that picture: what you see at (x, y) is
   where `click PID x y` lands.
4. **Look again** after anything that changes the page. Pages move: a banner,
   a notice or a dialog appears and what was at (x, y) is something else. A
   click on the wrong thing is worse than one more look.
5. **Say you are done**: `pleamar-wm agent done` when the task is over. While
   you work, the user's monitor glows; without it the glow waits a minute and
   a half in case you are only thinking.

## What works best

- **The keyboard before the mouse**, where there is a shortcut: in a browser
  `hotkey PID ctrl+l`, then `type` the address and `key enter` goes anywhere
  without hunting for the address bar. `ctrl+t`, `ctrl+w`, `ctrl+f` too.
- **Click a field before typing in it**, and look to check the caret is there.
- **Typing is ASCII only** for now: no accents, ñ or emoji. Write around it,
  or tell the user what they will have to finish by hand.
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
