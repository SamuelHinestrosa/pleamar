# The agent race

How much it costs an agent to use a pleamar window, measured at each step we
build ([note 12](../../docs/12-agents.md)), so that every step is shown to
improve on how computer use is done today, and so that what is still slow shows up.

**The task** (notes.plm): open a note from the list, make a new note, and check
each step on the status line. Since `wait` (step 3) it also saves, which takes
0.8 s as a disk or a network would, and checks it says «Saved». **A** is computer use as it is today: a picture,
a click where the picture says, another picture to check. **B** asks the window:
`describe`, act, `describe` to check.

## The hands (`race.py`)

Every command as a script runs it, without the model reading and deciding in
between. Times in ms, the two ways of seeing as medians of ten; tokens are what
the model has to read (a picture, about one per 750 pixels; text, one per four
characters).

### Open, new and save (from step 3, `wait`)

| Date | Commit | look | describe | A task | B task | A tokens (looks) | B tokens | Checked | Note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-06 | f39e725+ | 233 | 6 | 3602 | 1278 | 4039 (7 looks) | 285 | ✓ | wait |

### Open and new (steps 1 and 2)

| Date | Commit | look | describe | A task | B task | A tokens | B tokens | B acts by | Checked | Note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-06 | 9283a89+ | 220 | 6 | 1596 | 937 | 1731 | 577 | click | ✓ | describe, with loose texts |
| 2026-10-06 | 3c8606d+ | 280 | 6 | 1832 | 453 | 1731 | 243 | press | ✓ | press by name |

## With the model in the loop

An agent (Claude Code) does the task itself, both ways, in a live session; the
wall clock runs from its first command to its last, its own reading and
deciding included. Written by hand.

| Date | Commit | Model | A (pictures) | B (describe) | What made the difference |
| --- | --- | --- | --- | --- | --- |
| 2026-10-06 | 9283a89+ | Claude Opus 5.5 | 16.4 s, 3 pictures of 829×522 (~1700 tokens) | 7.2 s, 814 characters (~250 tokens) | No picture to read; each click and its check in one command. B still clicks at the centre of `describe`'s box: `press` does not exist yet |
| 2026-10-06 | 3c8606d+ | Claude Opus 5.5 | (as above: 16.4 s) | 3.0 s one command a step, 3.8 s with both presses in one; describe once (~200 tokens) and each `press` answering with what changed (~50 tokens) | `press` by name: no picture, no coordinates, no second describe to check |
| 2026-10-06 | f39e725+ | Claude Opus 5.5 | Open, new **and save**: 22.7 s, 5 pictures (~2900 tokens), clicking and looking in one command each | 5.4 s, one command a step: describe, three `press`, `wait status == "Saved"` (~350 tokens) | `wait`: the save is asked about once, not looked at until it shows |
