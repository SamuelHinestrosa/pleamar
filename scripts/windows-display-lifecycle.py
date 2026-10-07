"""Native GPU/display-lifecycle rehearsal with process-local output filtering.

Requires the ignored Rust test helper and a real Windows desktop. This changes
only which real monitor the test process observes, never Windows display state.
It validates runtime lifecycle, not physical hotplug or driver/DPI migration.
"""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import time
import ctypes as C
from ctypes import wintypes as W

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True, help='native pleamar executable for IPC')
parser.add_argument('--harness', type=Path, required=True, help='compiled pleamar library test executable')
parser.add_argument('--output', type=Path, required=True, help='new directory for evidence and isolated state')
parser.add_argument('--interactive', action='store_true', help='wait for an actual panel click after reconnection')
parser.add_argument('--monitor', help='explicit non-primary Windows display name')
parser.add_argument('--ci-owned-desktop', action='store_true', help='use the disposable GitHub-hosted desktop')
args = parser.parse_args()
assert os.name == 'nt', 'Native Windows rehearsal'
if args.ci_owned_desktop:
    assert all(os.environ.get(k)==v for k,v in [('GITHUB_ACTIONS','true'),('RUNNER_ENVIRONMENT','github-hosted'),('PLEAMAR_CI_DISPLAY_LIFECYCLE','1')])
else:
    assert args.monitor, 'Select an explicit secondary monitor'
root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('windows_smoke', root / 'scripts/windows-smoke.py')
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
binary, harness, out = args.binary.resolve(), args.harness.resolve(), args.output.resolve()
out.mkdir(parents=True, exist_ok=False)
scene = out / 'native-display-lifecycle.plm'
for suffix in ('.plm', '.luau'):
    scene.with_suffix(suffix).write_bytes((root / 'tests' / scene.with_suffix(suffix).name).read_bytes())
control = out / 'output-state.txt'

def connected(value):
    staging = control.with_suffix('.new')
    staging.write_text('connected' if value else 'absent', encoding='utf-8')
    staging.replace(control)

connected(False)
env = dict(os.environ, PLEAMAR_DISPLAY_TEST_SCENE=str(scene), PLEAMAR_DISPLAY_TEST_CONTROL=str(control),
    PLEAMAR_SOCKET_DIR=f'display-lifecycle-{os.getpid()}', PLEAMAR_NO_RELAUNCH='1',
    PLEAMAR_TEST_WINDOWS='1', APPDATA=str(out / 'state'))
if args.monitor: env['PLEAMAR_DISPLAY_TEST_MONITOR'] = args.monitor
if not args.ci_owned_desktop: env.pop('PLEAMAR_CI_DISPLAY_LIFECYCLE', None)
report = {'complete': False, 'phases': [], 'physical_input_requested': args.interactive,
    'harness_sha256': hashlib.sha256(harness.read_bytes()).hexdigest(),
    'ipc_binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'scene_sha256': hashlib.sha256(scene.read_bytes()).hexdigest()}
process = None
user=C.WinDLL('user32',use_last_error=True)
user.GetForegroundWindow.restype=W.HWND
user.GetWindowThreadProcessId.argtypes=[W.HWND,C.POINTER(W.DWORD)]


def guard():
    if process is not None and not args.interactive:
        pid=W.DWORD();user.GetWindowThreadProcessId(user.GetForegroundWindow(),C.byref(pid))
        assert pid.value!=process.pid, 'Owned lifecycle fixture took foreground'

def ask(command):
    return subprocess.check_output([str(binary), '--say', scene.stem, command], env=env,
        encoding='utf-8', stderr=subprocess.DEVNULL, timeout=5, creationflags=subprocess.CREATE_NO_WINDOW).strip()

def until(check, label, seconds=45):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        guard()
        if process.poll() is not None: raise RuntimeError(f'{label}: native process exited {process.returncode}')
        try:
            if check(): return
        except (subprocess.SubprocessError, ValueError): pass
        time.sleep(.15)
    raise RuntimeError(label)

def phase(name):
    guard()
    until(lambda: ask('get logic_screens') == ask('get screens.count'), 'Lua did not receive the native output count')
    state = {key: ask('get ' + key) for key in ('screens.count', 'logic_screens', 'screen_events', 'logic_monitor', 'hosted', 'ticks', 'clicks', 'open')}
    assert state['hosted'] == ('true' if state['screens.count']!='0' else 'false')
    state.update(name=name, native_hwnds=smoke.native_window_count(process.pid, visible_only=False))
    report['phases'].append(state)
    (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(state), flush=True)
    return state

try:
    with (out / 'native.log').open('w', encoding='utf-8') as log:
        process = subprocess.Popen([str(harness), '--exact', 'platform::windows::input_tests::native_display_lifecycle_helper', '--ignored', '--nocapture'],
            env=env, stdout=log, stderr=log, creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        until(lambda: int(ask('get ticks')) >= 8, 'Luau/IPC did not remain live without an output')
        initial = phase('initially absent')
        assert initial['native_hwnds'] == 0 and initial['screens.count'] == '0'
        connected(True)
        until(lambda: smoke.native_window_count(process.pid) == 4 and 'first frame' in (out / 'native.log').read_text(encoding='utf-8'), 'Native panels did not present')
        present = phase('two panels on one output')
        assert present['screens.count'] == '1', f"Two panels on one monitor reported screens.count={present['screens.count']}"
        assert present['screen_events'] == '1'
        if args.monitor: assert present['logic_monitor'].strip('"') == args.monitor
        ask('fact open true')
        until(lambda: smoke.native_window_count(process.pid) == 6, 'Popup did not open')
        assert phase('popup open')['screen_events'] == '1', 'A popup created a false monitor event'
        for cycle in range(3):
            connected(False)
            until(lambda: smoke.native_window_count(process.pid, visible_only=False) == 0, 'Removed HWNDs were not destroyed after renderer release')
            absent = phase(f'absent {cycle}')
            assert absent['screens.count'] == '0', 'Removed output retained a nonzero screens.count'
            assert int(absent['screen_events']) == 2 + cycle*2
            assert absent['open'] == 'false', 'A popup outlived its removed parent'
            previous_ticks = int(absent['ticks'])
            until(lambda: int(ask('get ticks')) >= previous_ticks + 4, 'Luau stopped after output removal')
            connected(True)
            until(lambda: smoke.native_window_count(process.pid) == 4, 'Panels did not recover')
            restored = phase(f'restored {cycle}')
            assert restored['screens.count'] == '1'
            assert int(restored['screen_events']) == 3 + cycle*2
            # Parent removal dismisses its popup. Reopen it explicitly against
            # the replacement parent instead of expecting an orphan to return.
            ask('fact open true')
            until(lambda: smoke.native_window_count(process.pid) == 6, 'Popup did not reopen on the replacement parent')
            assert phase(f'popup restored {cycle}')['screen_events'] == restored['screen_events']
        ask('fact open false')
        until(lambda: smoke.native_window_count(process.pid, visible_only=False) == 4, 'Popup resources were not released')
        assert phase('ready for input')['screen_events'] == '7', 'Closing a popup changed the monitor count'
        logic = scene.with_suffix('.luau')
        logic.write_text(logic.read_text(encoding='utf-8') + '\nfact.logic_reloaded = true\n', encoding='utf-8')
        until(lambda: ask('get logic_reloaded') == 'true', 'Lua did not reload')
        assert phase('Lua reloaded')['screen_events'] == '7'
        source = scene.read_text(encoding='utf-8')
        at = source.rfind('\n}')
        assert at >= 0
        scene.write_text(source[:at] + '\n    fact new_after_reload = true' + source[at:], encoding='utf-8')
        until(lambda: ask('get new_after_reload') == 'true', 'Scene did not reload')
        assert phase('scene reloaded')['screen_events'] == '7'
        if args.interactive:
            print('READY: click the main Monitor lifecycle panel after reconnection.', flush=True)
            until(lambda: int(ask('get clicks')) >= 1, 'Physical click after reconnection not received', seconds=120)
            phase('physical click after reconnection')
        ask('quit')
        process.wait(timeout=15)
        assert process.returncode == 0
        assert smoke.native_window_count(process.pid, visible_only=False) == 0
        report['complete'] = True
except BaseException as error:
    report['failure'] = str(error)
    raise
finally:
    if process is not None and process.poll() is None:
        try: ask('quit'); process.wait(timeout=15)
        except subprocess.SubprocessError: process.kill(); process.wait(); report['forced_cleanup'] = True
    report['exit_code'] = process.returncode if process is not None else None
    (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
