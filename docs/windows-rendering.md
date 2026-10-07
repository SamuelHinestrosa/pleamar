# Windows retained-surface experiment

`PLEAMAR_RETAINED_SURFACE=1` enables an experimental retained canvas for native
swapchain surfaces. It is off by default while native pixel and performance
comparisons are pending. It does not change Linux or provider-lent frames.

The swapchain's acquired texture cannot be assumed to contain the previous
frame. The experiment paints known damage into a separate canvas, clears that
region before blending, and copies the complete result to the swapchain. Unknown
damage still repaints everything. Glass uses its existing canvas. Reconfiguration,
closure and continuous full repaints retire retained canvases.

Requested retained BGRA texture capacity is limited to 32 MiB across the renderer.
This is extra memory traded for less painting; it is not a bound on driver memory
or a RAM-reduction claim. Allocation starts only for localized changes, and an
exhausted budget falls back to complete painting. `PLEAMAR_FULL_REPAINT=1` disables
retention for comparison; `PLEAMAR_TIMING=1` records allocations and releases.

The disposable Windows CI compares native screenshots against full repaint for
movement, opacity, blurred groups, reopening and resize. It never sends OS input.
The fixture refuses local execution. Passing its images does not establish
whole-Marea performance, mixed-DPI behavior or physical-GPU frame rates.
