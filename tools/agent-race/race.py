#!/usr/bin/env python3
"""The agent race: what it costs an agent to use a pleamar window by looking at
its pictures, against what it costs by asking it (`describe`, and `press` once
it exists). Run in a pleamar-wm session with `agent on`; it opens its own small
window (notes.plm) on a monitor you are not on, and closes it at the end.

    tools/agent-race/race.py [--note "what changed"] [--pleamar PATH]

It measures the hands only —every command, as a script would run it—, not the
model reading and deciding between them. That part is the live race: an agent
does the same task both ways and the wall clock is written down by hand in
results.md. See docs/12-agents.md.
"""

import argparse
import datetime
import json
import os
import re
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def run(*cmd):
    return subprocess.run(cmd, capture_output=True, text=True)


def timed(*cmd):
    a = time.perf_counter()
    r = run(*cmd)
    return (time.perf_counter() - a) * 1000, r


def median_ms(cmd, n=10):
    return statistics.median(timed(*cmd)[0] for _ in range(n))


def image_tokens(w, h):
    # What a picture costs a Claude model to read: about one token per 750 pixels.
    return round(w * h / 750)


def text_tokens(s):
    return round(len(s) / 4)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--note", default="", help="what changed since the last race")
    p.add_argument("--pleamar", default=os.path.join(ROOT, "target/dev/release/pleamar"))
    a = p.parse_args()
    pl = a.pleamar
    wm = ["pleamar-wm", "agent"]

    if run(*wm, "windows").returncode != 0:
        sys.exit("pleamar-wm agent does not answer: this needs a pleamar-wm session with `agent on`")
    sock = os.path.join(os.environ.get("PLEAMAR_SOCKETS", ""), "notes.sock")
    # A socket left by one that is gone does not count: the new one takes its place.
    alive = lambda: run(pl, "--say", "notes", "get status").returncode == 0
    if alive():
        sys.exit(f"a scene called notes is already running ({sock}): close it first")

    opened = run(*wm, "open", pl, "--scene", os.path.join(HERE, "notes.plm"), "--no-hud").stdout
    pid = re.search(r"process (\d+)", opened).group(1)
    for _ in range(60):
        line = next((l for l in run(*wm, "windows").stdout.splitlines() if l.split()[:1] == [pid] and "seen" in l), None)
        if line and alive():
            break
        time.sleep(0.25)
    else:
        sys.exit("the race's window did not open")
    time.sleep(1.0)
    say = lambda *w: run(pl, "--say", "notes", " ".join(w)).stdout
    shot = os.path.join(tempfile.mkdtemp(prefix="race-"), "look.png")

    try:
        # ── what each way of seeing costs ─────────────────────────────
        look_out = run(*wm, "look", pid, shot).stdout
        w, h = map(int, re.search(r"(\d+)x(\d+)", look_out.split()[-1]).groups())
        look_ms = median_ms([*wm, "look", pid, shot])
        tree = say("describe")
        describe_ms = median_ms([pl, "--say", "notes", "describe"])
        scale = float(re.search(r"scale ([\d.]+)", tree.splitlines()[0]).group(1))
        nodes = {n.get("name"): n for part in json.loads(say("describe json")) for n in walk(part["nodes"])}
        press_by_name = not say("press nothing-at-all").startswith("? I don't understand")

        # Both ways wait the same for the window to answer a click —the logic
        # changing the status line— before looking again; what is timed is the
        # looking. (`wait`, step 3, is what will let B do this by itself.)
        def settled(says):
            for _ in range(100):
                if says in say("get status"):
                    return True
                time.sleep(0.01)
            return False

        def centre(name):
            x, y, bw, bh = nodes[name]["box"]
            return str(round((x + bw / 2) * scale)), str(round((y + bh / 2) * scale))

        # ── the task, both ways: open a note, make a new one, check each ──
        # A: as computer use does today. The places to click are the ones a
        # model would read off the picture; here they are given, so only the
        # hands are timed.
        a0 = time.perf_counter()
        run(*wm, "look", pid, shot)
        run(*wm, "click", pid, *centre("hit#r1"))
        settled("Opened: Shopping list")
        run(*wm, "look", pid, shot)
        run(*wm, "click", pid, *centre("new"))
        settled("New note created")
        run(*wm, "look", pid, shot)
        a_ms = (time.perf_counter() - a0) * 1000
        a_tokens = 3 * image_tokens(w, h)

        # B: asking the window. With `press`, its answer is what happened:
        # nothing to look at again.
        b0 = time.perf_counter()
        read = say("describe")
        if press_by_name:
            read += say("press", "hit#r2")
            ok_open = "Opened: pleamar ideas" in read
            last = say("press", "new")
        else:
            run(*wm, "click", pid, *centre("hit#r2"))
            settled("Opened: pleamar ideas")
            read += say("describe")
            ok_open = "Opened: pleamar ideas" in read
            run(*wm, "click", pid, *centre("new"))
            settled("New note created")
            last = say("describe")
        read += last
        b_ms = (time.perf_counter() - b0) * 1000
        b_tokens = text_tokens(read)
        ok_new = "New note created" in last
    finally:
        say("quit")
        run(*wm, "done")

    commit = run("git", "-C", ROOT, "rev-parse", "--short", "HEAD").stdout.strip()
    dirty = "+" if run("git", "-C", ROOT, "status", "--porcelain", "--", "src").stdout.strip() else ""
    row = (
        f"| {datetime.date.today()} | {commit}{dirty} | {look_ms:.0f} | {describe_ms:.0f} | {a_ms:.0f} | {b_ms:.0f} "
        f"| {a_tokens} | {b_tokens} | {'press' if press_by_name else 'click'} | {'✓' if ok_open and ok_new else '✗'} | {a.note} |"
    )
    print(f"window {w}×{h} at scale {scale}")
    print(f"seeing:  look {look_ms:.0f} ms ({image_tokens(w, h)} tokens)  ·  describe {describe_ms:.0f} ms ({text_tokens(tree)} tokens)")
    print(f"task:    A pictures {a_ms:.0f} ms, {a_tokens} tokens  ·  B describe {b_ms:.0f} ms, {b_tokens} tokens, acting by {'press' if press_by_name else 'click'}")
    print(f"checked: opened {'✓' if ok_open else '✗'}  new note {'✓' if ok_new else '✗'}")
    # Under the last row of the hands' table.
    path = os.path.join(HERE, "results.md")
    lines = open(path).read().split("\n")
    start = next(i for i, l in enumerate(lines) if l.startswith("| Date | Commit | look |"))
    end = next(i for i in range(start + 2, len(lines) + 1) if i == len(lines) or not lines[i].startswith("|"))
    lines.insert(end, row)
    open(path, "w").write("\n".join(lines))
    print("written to results.md")


def walk(nodes):
    for n in nodes:
        yield n
        yield from walk(n.get("children", []))


if __name__ == "__main__":
    main()
