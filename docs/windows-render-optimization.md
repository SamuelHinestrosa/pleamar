# Renderer optimization — 2026-10-03

Developed with Codex. This follow-up keeps the existing frame rate, effects and
input semantics. It does not declare complete Windows desktop parity.

## Changes

Hit testing and native input regions share evaluated zone geometry. A zone keeps
its flattened curves, transforms, viewport clips and bounds until a property,
spring velocity or fact used by that geometry changes. Active/hidden checks
still run normally. Each scene load replaces the caches, including reloads with
the same number of zones. Storage is bounded by the current scene's zones and
path limits; there is no history of old shapes.

The composition snapshot reuses its vectors and unchanged text strings instead
of allocating a new copy on every animation frame. Scene reload also explicitly
invalidates the previous draw list: a changed constant must repaint even if all
live property/fact values are unchanged.

## Verification

The unit regressions compare cached and original geometry across ellipses,
rectangles, arcs, segments, curves, strokes, rotations, nested transforms and
two polygon viewport clips. They vary property values, velocity, facts and
collapsed dimensions, and replace constant geometry. Native regression input
goes directly into the renderer: no desktop mouse or keyboard is injected.

```powershell
cargo test --release --locked --lib
python scripts/run-tests.py --binary target/release/pleamar.exe
python scripts/windows-zone-cache.py --binary target/release/pleamar.exe --screen '\\.\DISPLAY2'
$env:PLEAMAR_ZONE_BENCH_SCENE = '../marea-plm/marea-desktop.plm'
cargo test --release --locked --lib scene_geometry_benchmark -- --ignored --nocapture
```

The native test uses a panel on the specified monitor, with no keyboard focus,
work-area reservation or system services. Exactly three scripted clicks must
reach it; clicks at its old position, while disabled, and at the old position
after a hot reload must not. It also checks the output names and successful
Luau/runtime shutdown. This is a native rendering/input test, not a screenshot
review or physical input test.

## Measurements

Windows 11 x64/MSVC, RTX 5070, default Luau. A game remained running on the
primary display. Compiles used below-normal priority and two Cargo jobs.

An isolated release benchmark evaluates bounds and hit tests for all 586 Marea
zones, 300 rounds per case. The unchanged and changing paths must agree with the
original implementation. One run measured:

| Geometry | Original, µs/round | Cached, µs/round |
|---|---:|---:|
| Unchanged | 504.72 | 41.45 |
| Every property changes every round | 424.83 | 316.75 |

These are geometry costs, not total application CPU or display FPS. The native
comparison uses the same generated Marea drawing on the secondary display,
Classic look, two 20-second samples each with its panel closed and open. It uses
isolated state and a minimal Luau driver, without Marea's desktop commands or
user preferences; the five declared read-only services remain active. Other
surfaces are closed, keyboard requests and work-area reservation are disabled,
and `--screen` confines creation to the selected output. The ordinary installed
Marea stays running throughout both variants.

The final native comparison completed with no runtime errors and only DISPLAY2
surfaces. Means of the two samples, expressed as a percentage of one CPU core:

| Panel | Previous engine | Updated engine |
|---|---:|---:|
| Closed | 13.91% | 14.50% |
| Open | 22.97% | 21.21% |

Update means stayed around 16.7 ms. The open-panel samples improved, but the
closed-panel samples did not; this does not establish a general CPU percentage
saving. Final resident/private process samples were 271.20/335.92 MiB before
and 284.22/368.84 MiB after. There is **no demonstrated RAM reduction** in this
comparison. The geometry cache is a small bounded addition; process/driver and
allocator variation cannot be attributed to it from these short samples.

The final MSVC/default-Luau build passed 126 library tests (26 opt-in tests
ignored), 227 language/documentation checks and 15 Marea logic suites. The
secondary-monitor native input/reload regression passed. The original native
test attempt selected an absent output because the test generator used JSON
escaping for a PLM literal; correcting that test literal resolved it. No product
failure was inferred from that attempt.

Marea's `windows/measure-desktop.py --screen '\\.\DISPLAY2'` records the actual
surface names and rejects a run that creates a surface on another output.
The fixture and measurement are reproducible with:

```powershell
python scripts/windows-render-benchmark.py --marea ../marea-plm --binary <engine.exe> --screen '\\.\DISPLAY2' --output <new-evidence-directory>
```

Update intervals and process CPU are recorded separately. Concurrent gameplay
and driver/allocator caches make these short samples unsuitable for a general
RAM-saving or game-FPS claim. No working-set trimming or lower animation rate
is used.
