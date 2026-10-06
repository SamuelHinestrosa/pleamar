# The agent race

How much it costs an agent to use a pleamar window, measured at each step we
build ([note 12](../../docs/12-agents.md)), so that every step is shown to
improve on how computer use is done today, and so that what is still slow shows up.

**The task** (notes.plm): open a note from the list, make a new note, and check
each step on the status line. **A** is computer use as it is today: a picture,
a click where the picture says, another picture to check. **B** asks the window:
`describe`, act, `describe` to check.

## The hands (`race.py`)

Every command as a script runs it, without the model reading and deciding in
between. Times in ms, the two ways of seeing as medians of ten; tokens are what
the model has to read (a picture, about one per 750 pixels; text, one per four
characters).

| Date | Commit | look | describe | A task | B task | A tokens | B tokens | B acts by | Checked | Note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-06 | 9283a89+ | 220 | 6 | 1596 | 937 | 1731 | 577 | click | ✓ | describe, with loose texts |

## With the model in the loop

An agent (Claude Code) does the task itself, both ways, in a live session; the
wall clock runs from its first command to its last, its own reading and
deciding included. Written by hand.

| Date | Commit | Model | A (pictures) | B (describe) | What made the difference |
| --- | --- | --- | --- | --- | --- |
| 2026-10-06 | 9283a89+ | Claude Opus 5.5 | 16.4 s, 3 pictures of 829×522 (~1700 tokens) | 7.2 s, 814 characters (~250 tokens) | No picture to read; each click and its check in one command. B still clicks at the centre of `describe`'s box: `press` does not exist yet |
