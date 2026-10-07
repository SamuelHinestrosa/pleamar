# Native scene commands

This work integrates upstream `4a51a07`: named actions, wait/watch, hello,
language 0.3 and explicit control roles, values and states. Windows validation
of the final input-guard correction is in progress. The preceding
shared-capture CI and preview.34 installer do not validate this integration.

The Windows client supports `describe`, `describe json`, `press`, `hold`,
`drag`, `wheel`, `type`, `key`, `wait`, `watch` and `hello`, alongside the
existing scene commands. Actions go through the scene's own render/input
logic. They do not inject the system mouse or keyboard and do not implement
an independent desktop seat.

For a running scene named `notes`, from PowerShell:

```powershell
pleamar --say notes 'hello'
pleamar --say notes 'describe json'
pleamar --say notes 'press save'
pleamar --say notes 'type query España ñ 世界'
pleamar --say notes 'wait dirty == false 3s'
pleamar --say notes 'watch 10s'
```

Use the actual element names returned by `describe`. The matching Windows WM
also exposes `agent scenes`, `agent tree PID [json]`, `agent press PID NAME`,
`agent wait PID CONDITION`, `agent watch PID [SECONDS]` and `agent say PID
ORDER...`. `agent scenes` includes panels that the ordinary native window
catalog excludes. `scene:ENDPOINT` selects an endpoint explicitly when a
process has more than one scene. Discovery verifies the reported PID against
the connected Windows pipe's native server PID. An action reconnects and
checks that PID before sending its order, so an endpoint replaced by another
process cannot receive a stale action. Generic application click/type/open and the
Linux independent cursor remain unsupported and fail explicitly.

## Lifetime and transport

The scene namespace and pipe DACL remain scoped to the current Windows logon.
Eight command workers can run concurrently, with a separate listening instance.
An idle listener sleeps in the kernel. A wait or subscription therefore does
not serialize unrelated commands. Excess requests receive an explicit busy
response; `quit` remains available when all worker slots are occupied.

New clients opt into streaming with the `@stream-v1 ` prefix. Each response
is one JSON string followed by a newline; `null` ends the response. Clients
acknowledge each frame with `ack\n`. This preserves Unicode and embedded
newlines and prevents disconnect from discarding the last reply. Frames are
bounded to 64 KiB. A stalled write or acknowledgement has a two-second deadline.
Legacy clients retain the original one-reply protocol; they must upgrade to
use `watch`. Notification activation pipes retain their separate single-client
protocol and namespace.

`watch` and `wait` check connection liveness while waiting. A disconnected
client releases its renderer lease and wakes an idle scene to retire the
subscription. Reload retires pending conditions, watches and actions with an
explicit message: their stored indices belong to the old scene. Query the
new description before repeating an action. Invalid/non-finite drag and wheel
arguments are rejected before entering the input path.

Queued and active named actions also hold connection leases. Disconnect or a
command deadline cancels pending steps and clears a held virtual drag without
synthesizing a release action. Other connected clients can continue issuing
commands. This cannot undo an action that already ran before cancellation.

Named presses and wheel actions recheck the actual input point immediately
before the effect. A hover or animation may have covered it since the initial
description. A reachable corner elsewhere in the target does not authorize
pressing a different element at the old point. Typing and keys also recheck
that the focused field remains visible and available to agents. Hidden,
person-only, lock and capture-excluded surfaces cannot be named targets.

The pipe ACL isolates Windows logons, not applications in the same logon.
Agent metadata describes intended interaction; it does not turn the legacy
fact/text/event command protocol into an authorization boundary.

## Validation record

Local Windows x64/MSVC validation before the final input guard (`v3`) passed
212 engine unit tests, 20 WM unit tests, 249 language checks, 35 documentation
scenes, the isolated Luau runner, and Marea's generated-profile/logic suites.
Default Luau was enabled. The native fixtures used only their own
non-activating windows on non-primary `DISPLAY2`, at 125% scaling:

- Named press/type/key/drag/wheel/hold, Unicode text, Luau callbacks,
  concurrent wait/watch, hot reload and clean closure passed.
- A filename containing the hello protocol's delimiter words routed to the
  correct native PID. Disconnected queued/held actions produced no delayed
  effect or synthetic release.
- The current generated Marea control center exposed Spanish labels,
  slider values and checked toggles. Its volume drag reached an isolated
  Luau handler. This fixture denied all services and did not change device
  volume or test live hardware integrations.

Two further native regressions failed on `v3`: a person-only overlay opened
between pointing and pressing received the press, and an already-focused
field accepted a key after an overlay covered it. Their captures showed the
unexpected effects. The final input guard addresses both; its build and native
reruns must pass before publication.

The local records are `agent-integration-build-20261006-v3.json`,
`agent-language-20261006-v3.json`, `marea-agent-profile-20261007-v3a.json`, and
the native `report.json` files under `agent-integration-native-20261006-v3`,
`marea-agent-metadata-20261007-v3`, `agent-moving-guard-20261007-v3`, and
`agent-field-guard-20261007-v3a`. Captured actions/reload/Marea controls and
both failed-regression images were inspected. These are local evidence,
not files needed by the installed application.

Checks to complete before publication:

- Run the complete engine and WM test suites with default Luau, including the
  pipe concurrency, Unicode, cancellation, saturation and shutdown regressions.
- Exercise named actions, concurrent wait/watch, Luau and reload through a
  real passive scene on the secondary monitor; inspect the captured result.
- Validate WM scene discovery and routing, including ambiguous identities.
- Run the language/Luau checks and exact-head Windows/Linux CI. Rebuild the
  paired installer only after those sources are ready.
