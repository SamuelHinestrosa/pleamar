# Primary monitor query

`sys.ask_async("window.primary", {}, callback)` returns the current Windows GDI
device name of the primary monitor, for example `\\\\.\\DISPLAY1`. It uses
`MonitorFromWindow(NULL, MONITOR_DEFAULTTOPRIMARY)` and `GetMonitorInfoW` without
walking the application catalogue or depending on foreground focus. Query again
when opening a primary-monitor UI so a display-settings change is respected.

Permission: `window.*`. No arguments are accepted. An unavailable native monitor
returns an error rather than silently choosing the foreground monitor. Marea's
window overview uses this service; the regular dock keeps its home-monitor scope.
