# Native Windows port: validation and remaining work

This branch is a work in progress based on pleamar `0728337` (0.2.3). Marea's
native desktop profile is based on `k4ditano/marea-plm` commit `9192f50`.
It is not a claim of complete Windows parity or readiness to merge.

## Original PR review

[PR #1](https://github.com/k4ditano/pleamar/pull/1), head
`3b6812e17a976a8f90b55fa32dac38e1663f39c3`, provided a useful Win32,
DirectComposition, monitor-placement and AppBar starting point. Its six changed
files were reviewed. The author reports cross-compilation without Luau and
limited runtime tests; those are not default-feature validation of this branch.
Popups, desktop services, Unix assumptions and lifecycle/input handling needed
additional work. The last GitHub inspection reported the PR unmergeable and
returned no comments, reviews or checks. Its reported hardware results were not
repeated on the local Windows host.

## Executed locally, September 30, 2026

Windows 11 x64, MSVC, Rust 1.98.1; RTX 5070, two monitors at 125% scaling.
The default-Luau release currently installed for local testing has SHA-256
`7c141910f898ef3ee54cad62c8f464d8f4ebe7a428908c31f50d91f26dec31be`.
Configuration, logs and detailed historical measurements remain local and are
not included in this source-validation branch.

| Command/check | Observed result |
|---|---|
| `cargo build --release --locked --bin pleamar --example luau-test` | Passed natively with default Luau; six existing warnings |
| `cargo test --locked` | 103 unit tests and one integration passed; 24 opt-in helpers ignored |
| `cargo test --locked --lib platform::windows::input_tests -- --nocapture` | Eleven hidden-window tests passed with the final helper; GPU/display helper ignored |
| `python scripts/run-tests.py --binary target/release/pleamar.exe` | 225 checks passed, including 31 documentation scenes |
| `python scripts/windows-smoke.py --binary target/release/pleamar.exe` | Native CLI, Unicode PowerShell autostart, absent-output Luau/IPC, timed and requested shutdown passed; GUI mode not run in this continuation |
| Marea `python windows/test-logic.py --binary ../pleamar/target/release/pleamar.exe --luau-runner ../pleamar/target/release/examples/luau-test.exe` | Generated profile and twelve isolated logic suites passed |
| Marea canonical installer and updater | PowerShell 5.1/7 reject a real no-Luau binary before touching a running installation; default-Luau update passed with backup and initialization |

Earlier native desktop sessions verified window rendering, pointer/keyboard,
Unicode text/clipboard, popups, click-through, saved-file hot reload and Marea
pages on earlier profile revisions. The current upstream rain-layout integration
has not completed visual review. A successful absent-output or hidden-window
fixture is not evidence that the current full interface works correctly.

The tests include reproducing negative controls for viewport hit testing,
compiler ownership, declarative services, watcher/permission isolation,
asynchronous callbacks, plugin approval and subprocess cleanup. Windows CLI
rehearsals exercise actual saved-file reloads in isolated namespaces. The latest
AppBar tests cover notification dispatch and retirement using real hidden HWNDs;
they do not restart Explorer or reserve desktop work area.

Core Audio subscriptions replace frequent volume polling. Read-only local
snapshots and registration/drop cycles passed, but physical volume/mute/default
changes and service-restart recovery still need validation with the new watcher.
Short CPU observations are not an end-to-end FPS or latency benchmark. A previous
long performance sample was invalidated because its panel state changed; the
verifier now rejects such traces.

## Not yet established

- Current-profile graphical review and controlled sustained CPU/GPU/memory and
  interaction-latency measurements, including cold start and recovery.
- Mixed DPI, physical monitor changes, IME, accessibility and resume.
- Actual Explorer restart and reserved work-area recovery with the new AppBar
  handler; autohide/taskbar interaction.
- Wi-Fi join/password/persistence on a physical adapter; this host has none.
- New Bluetooth pairing/PIN/cancellation and generic non-audio profile control.
- Internal-panel WMI brightness on a laptop; current monitor DDC reads report
  unavailable, despite an earlier successful supported-monitor test.
- HDR/4K/long recording and device changes; multi-monitor screenshot cases.
- Protected tray icons, notification action/retention limits and cross-process
  drag/drop coverage. See [capabilities](windows.md) for the implemented APIs and
  explicit limits.
- Deriva storage/ingestion/search: the indispensable external `deriva-worker`
  source/native distribution has not been located. No substitute database is
  presented as that service.
- Live Claude quota validation with an authenticated provider.
- Linux execution and remote Windows/Linux CI. Both jobs are defined; results
  must be recorded from actual runs before claiming they passed.
- Full final line review and focused upstream PRs. These validation branches
  exist to collect CI evidence and are not a ready-to-merge submission.

Marea's compositor rain/snow/window effects require the independent Linux
pleamar-wm project. The Windows desktop profile explicitly reports those effects
unavailable and retains its native brightness, output and input controls.

AI assistance: this implementation and its validation tools were developed with
Codex. Remaining review and validation gates are stated above.
