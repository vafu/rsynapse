"""Opt-in real compositor integration; run under dbus-run-session.

Requires Umbriel, Python GI/GTK4 and a built workspace-dbus binary. Both the
headless compositor and the projection use an isolated runtime directory/bus.
"""
import argparse
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
from gi.repository import Gio, GLib

BUS = 'org.rsynapse.Workspace'
ROOT = '/org/rsynapse/Workspace'
MANAGER = 'org.rsynapse.Workspace.Manager1'
OUTPUT = 'org.rsynapse.Workspace.Output1'
WORKSPACE = 'org.rsynapse.Workspace.Workspace1'
WINDOW = 'org.rsynapse.Workspace.Window1'


def client():
    import gi
    gi.require_version('Gtk', '4.0')
    from gi.repository import Gtk
    app = Gtk.Application(application_id='org.rsynapse.UmbrielProbe', flags=Gio.ApplicationFlags.NON_UNIQUE)
    windows = []
    def activate(app):
        for title in ['Umbriel probe A', 'Umbriel probe B']:
            window = Gtk.ApplicationWindow(application=app, title=title)
            window.set_default_size(320, 240)
            window.present()
            windows.append(window)
    app.connect('activate', activate)
    return app.run([])


def exercise(args, directory):
    runtime = directory / 'runtime'
    runtime.mkdir(mode=0o700)
    config = directory / 'config.toml'
    config.write_text('''[general]
autostart = []
xwayland = false
[output.HEADLESS-1]
position = [0, 0]
workspaces = ["1", "2"]
[output.HEADLESS-2]
position = [1920, 0]
workspaces = ["1", "2"]
''')
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), XDG_CURRENT_DESKTOP='umbriel',
               XDG_SESSION_DESKTOP='umbriel', XDG_SESSION_TYPE='wayland', WLR_BACKENDS='headless',
               WLR_LIBINPUT_NO_DEVICES='1', WLR_HEADLESS_OUTPUTS='2', GTK_A11Y='none', NO_AT_BRIDGE='1',
               GTK_USE_PORTAL='0', GDK_DEBUG='no-portals', GIO_USE_VFS='local')
    env.pop('WAYLAND_DISPLAY', None)
    env.pop('DISPLAY', None)
    env.pop('UMBRIEL_SOCKET', None)
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    waiting = {'predicate': None, 'value': None, 'loop': None}
    ready = {'ipc': False}
    processes = []
    logs = []
    subscriptions = []

    def wake(*_):
        predicate = waiting['predicate']
        if predicate is not None:
            try:
                value = predicate()
                if value:
                    waiting['value'] = value
                    waiting['loop'].quit()
            except (GLib.Error, ConnectionError, OSError):
                pass

    def wait(predicate):
        try:
            value = predicate()
        except (GLib.Error, ConnectionError, OSError):
            value = None
        if value:
            return value
        waiting.update(predicate=predicate, value=None, loop=GLib.MainLoop())
        def timeout():
            waiting['loop'].quit()
            return False
        timer = GLib.timeout_add_seconds(20, timeout)
        waiting['loop'].run()
        value = waiting['value']
        waiting['predicate'] = None
        if value:
            GLib.source_remove(timer)
        assert value, 'real Umbriel transition was not observed'
        return value

    subscriptions.append(connection.signal_subscribe(BUS, None, None, None, None,
                                                      Gio.DBusSignalFlags.NONE, wake))
    subscriptions.append(connection.signal_subscribe('org.freedesktop.DBus', 'org.freedesktop.DBus',
        'NameOwnerChanged', '/org/freedesktop/DBus', BUS, Gio.DBusSignalFlags.NONE, wake))
    native_socket = runtime / 'umbriel-wayland-0.sock'

    def request(cmd, **fields):
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(5)
            stream.connect(str(native_socket))
            stream.sendall(json.dumps(dict(cmd=cmd, **fields)).encode() + b'\n')
            reply = json.loads(stream.makefile().readline())
            assert 'ok' in reply, reply
            return reply['ok']

    def properties(path, interface):
        return connection.call_sync(BUS, path, 'org.freedesktop.DBus.Properties', 'GetAll',
            GLib.Variant('(s)', (interface,)), None, Gio.DBusCallFlags.NO_AUTO_START, 5000, None).unpack()[0]

    def inventory():
        root = properties(ROOT, MANAGER)
        return root, {w['Id']: w for path in root['Windows'] for w in [properties(path, WINDOW)]}

    def verify():
        root, windows = inventory()
        workspaces = {w['Id']: w for path in root['Workspaces'] for w in [properties(path, WORKSPACE)]}
        outputs = {o['Name']: o for path in root['Outputs'] for o in [properties(path, OUTPUT)]}
        nw = {w['id']: w for w in request('windows')}
        nws = {w['id']: w for w in request('workspaces')}
        no = {o['name']: o for o in request('outputs')}
        assert root['Connected']
        assert windows.keys() == nw.keys()
        assert workspaces.keys() == nws.keys()
        assert outputs.keys() == no.keys()
        for id, window in windows.items():
            native = nw[id]
            assert window['Title'] == ([native['title']] if native['title'] else [])
            assert window['Pid'] == ([native['pid']] if native['pid'] > 0 else [])
            position = native['column'] >= 0 and native['row'] >= 0 and not native['floating']
            assert window['ColumnIndex'] == ([native['column']] if position else [])
            assert window['RowIndex'] == ([native['row']] if position else [])
            if native['workspace']:
                assert next(w['Id'] for path in window['Workspace'] for w in [properties(path, WORKSPACE)]) == native['workspace']
            else:
                assert not window['Workspace']
            if native['output']:
                assert next(o['Name'] for path in window['Output'] for o in [properties(path, OUTPUT)]) == native['output']
        for id, workspace in workspaces.items():
            native = nws[id]
            assert workspace['Index'] == native['index']
            assert workspace['Name'] == ([native['name']] if native['named'] else [])
        focused = next((w for w in nw.values() if w['active']), None)
        if focused:
            assert next(properties(path, WINDOW)['Id'] for path in root['FocusedWindow']) == focused['id']
            assert next(properties(path, OUTPUT)['Name'] for path in root['FocusedOutput']) == focused['output']
        return root, windows

    try:
        log = (directory / 'compositor.log').open('wb')
        logs.append(log)
        compositor = subprocess.Popen([args.umbriel, '-c', str(config)], env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
        processes.append(compositor)
        pending = bytearray()
        def output(fd, condition):
            data = os.read(fd, 65536)
            if not data:
                wake()
                return False
            log.write(data)
            log.flush()
            pending.extend(data)
            while b'\n' in pending:
                line, _, tail = pending.partition(b'\n')
                pending[:] = tail
                if b'IPC listening on' in line:
                    ready['ipc'] = True
            wake()
            return True
        GLib.io_add_watch(compositor.stdout.fileno(), GLib.IO_IN | GLib.IO_HUP, output)
        wait(lambda: ready['ipc'])
        env.update(UMBRIEL_SOCKET=str(native_socket), WAYLAND_DISPLAY='wayland-0')
        producer_log = (directory / 'producer.log').open('wb')
        logs.append(producer_log)
        # The auto route must prefer the Umbriel desktop over an inherited NIRI_SOCKET.
        producer = subprocess.Popen([args.producer, '--backend', 'auto'], env=env,
            stdout=producer_log, stderr=subprocess.STDOUT)
        processes.append(producer)
        wait(lambda: properties(ROOT, MANAGER)['Connected'])
        request_stream = socket.socket(socket.AF_UNIX)
        request_stream.settimeout(10)
        request_stream.connect(str(native_socket))
        request_stream.sendall(b'{"cmd":"subscribe","events":["windows"]}\n')
        with request_stream, request_stream.makefile() as events:
            probe = subprocess.Popen([sys.executable, __file__, '--client'], env=dict(env, GDK_BACKEND='wayland'))
            processes.append(probe)
            while True:
                frame = json.loads(events.readline())
                if frame['event'] == 'windows' and len(frame['data']) == 2:
                    break
        wait(lambda: len(inventory()[1]) == 2)
        root, windows = verify()
        assert len(root['Outputs']) == 2
        b = next(w['id'] for w in request('windows') if w['title'] == 'Umbriel probe B')
        request('msg', arg=f'window-focus:{b}')
        app_id = next(w['app_id'] for w in request('windows') if w['id'] == b)
        subprocess.run(['bash', args.url_helper, 'https://example.invalid/umbriel-probe'], check=True,
            env=dict(env, RSYNAPSE_URL_BROWSER='/usr/bin/true',
                     RSYNAPSE_URL_BROWSER_DESKTOP='probe.desktop',
                     RSYNAPSE_URL_BROWSER_APP_ID_RE='^' + re.escape(app_id) + '$'))
        assert next(w['id'] for w in request('windows') if w['active']) == b
        request('msg', arg='window-move-to-workspace:1/HEADLESS-2')
        wait(lambda: next((o['Name'] for path in inventory()[1][b]['Output'] for o in [properties(path, OUTPUT)]), None) == 'HEADLESS-2')
        verify()
        for mode in ['dwindle', 'master', 'scrolling']:
            request('msg', arg=f'workspace-set-layout:{mode}')
            target = next(w['id'] for w in request('workspaces') if w['focused'])
            members = [w['id'] for w in request('windows') if w['workspace'] == target]
            wait(lambda: all(bool(inventory()[1][id]['ColumnIndex']) == (mode == 'scrolling') for id in members))
            verify()
        request('msg', arg=f'window-toggle-floating:{b}')
        wait(lambda: inventory()[1][b]['Floating'])
        verify()
        request('msg', arg=f'window-focus:{b}')
        request('msg', arg='window-move-to-scratchpad')
        wait(lambda: not inventory()[1][b]['Workspace'])
        verify()
        request('msg', arg='scratchpad-toggle')
        wait(lambda: any(properties(path, WINDOW)['Id'] == b for path in properties(ROOT, MANAGER)['FocusedWindow']))
        assert not properties(ROOT, MANAGER)['FocusedWorkspace']
        verify()
        request('output-create', arg='api-probe 800x600@30')
        wait(lambda: len(properties(ROOT, MANAGER)['Outputs']) == 3)
        verify()
        request('output-destroy', arg='api-probe')
        wait(lambda: len(properties(ROOT, MANAGER)['Outputs']) == 2)
        verify()
        print(json.dumps({'real_umbriel': True, 'outputs': 2, 'windows': 2,
            'layouts': ['scrolling', 'dwindle', 'master'], 'floating': True,
            'scratchpad': True, 'output_lifecycle': True, 'url_helper': True, 'backend': 'auto',
            'inherited_niri_socket': bool(env.get('NIRI_SOCKET'))}))
    finally:
        for sub in subscriptions:
            connection.signal_unsubscribe(sub)
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for log in logs:
            log.close()


if __name__ == '__main__':
    if '--client' in sys.argv:
        sys.exit(client())
    parser = argparse.ArgumentParser()
    parser.add_argument('--producer', required=True)
    parser.add_argument('--umbriel', default='umbriel')
    parser.add_argument('--artifacts', required=True)
    parser.add_argument('--url-helper', default=str(Path(__file__).resolve().parents[3] / 'install/bin/rsynapse-open-url'))
    args = parser.parse_args()
    directory = Path(args.artifacts)
    directory.mkdir(mode=0o700)
    exercise(args, directory)
