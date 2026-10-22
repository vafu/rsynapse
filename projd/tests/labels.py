"""Project-owned labels: old stores, API/signals, CLI, races and restart, private bus only."""
import concurrent.futures
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from gi.repository import Gio, GLib

ROOT = pathlib.Path(__file__).resolve().parents[2]
PROJD = os.environ.get('PROJD_BIN', str(ROOT / 'steward/target/debug/rsynapse-projd'))
PROJ = os.environ.get('PROJ_BIN', str(ROOT / 'steward/target/debug/proj'))
BUS = 'org.rsynapse.Proj'
PATH = '/org/rsynapse/Proj'
MANAGER = BUS + '.Manager1'
PROJECT = BUS + '.Project1'
PROPERTIES = 'org.freedesktop.DBus.Properties'

if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='projd-labels-') as temp:
        env = dict(os.environ, PROJD_STORE_PATH=temp + '/store.sqlite3', TEST_ROOT=temp,
                   XDG_DATA_HOME=temp, XDG_DATA_DIRS=temp)
        subprocess.run(['dbus-run-session', '--', sys.executable, __file__, '--child'],
                       env=env, check=True)
    sys.exit(0)


def cli(*args, cwd=None):
    return subprocess.check_output([PROJ, *args], text=True, cwd=cwd)


def fails(*args, cwd=None):
    result = subprocess.run([PROJ, *args], capture_output=True, text=True, cwd=cwd)
    assert result.returncode != 0, result
    return result.stderr


def connection():
    return Gio.DBusConnection.new_for_address_sync(
        os.environ['DBUS_SESSION_BUS_ADDRESS'],
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)


def invoke(conn, path, interface, method, signature='()', args=()):
    return conn.call_sync(BUS, path, interface, method, GLib.Variant(signature, args),
                          None, Gio.DBusCallFlags.NONE, 5000, None).unpack()


def call(method, signature='()', args=(), conn=None):
    return invoke(conn or connection(), PATH, MANAGER, method, signature, args)


def labels(method, values, id=None, conn=None):
    return list(call(method, '(sas)', (id or project_id, values), conn)[0])


def prop(name, path=None):
    return invoke(client, path or project_path, PROPERTIES, 'Get', '(ss)', (PROJECT, name))[0]


def objects():
    return invoke(client, PATH, 'org.freedesktop.DBus.ObjectManager', 'GetManagedObjects')[0]


def ready():
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if subprocess.run([PROJ, 'project', 'list', '--json'], capture_output=True).returncode == 0:
            return
    raise AssertionError('daemon did not become ready')


def pump_until(predicate):
    deadline = time.monotonic() + 5
    while not predicate():
        assert time.monotonic() < deadline, 'signal not received'
        GLib.MainContext.default().iteration(False)


base = pathlib.Path(os.environ['TEST_ROOT'])
directory = base / 'legacy'
directory.mkdir()
project_id = '0123456789abcdef0123456789abcdef'
project_path = PATH + '/Projects/p' + project_id
old = dict(id=project_id, name='Legacy', cwd=str(directory), checkout_id='', icon='', icon_origin='')
with sqlite3.connect(os.environ['PROJD_STORE_PATH']) as db:
    db.execute('CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id))')
    db.execute('INSERT INTO records VALUES(?,?,?)', ('project', project_id, json.dumps(old)))

daemon = subprocess.Popen([PROJD])
try:
    ready()
    client = connection()
    assert prop('Labels') == []
    assert objects()[project_path][PROJECT]['Labels'] == []
    with sqlite3.connect(os.environ['PROJD_STORE_PATH']) as db:
        assert db.execute("SELECT count(*) FROM records WHERE kind='project-labels'").fetchone()[0] == 0

    # Additive introspection contract; all old ProjectInfo method signatures remain intact.
    xml = ET.fromstring(invoke(client, PATH, 'org.freedesktop.DBus.Introspectable', 'Introspect')[0])
    manager = xml.find(f"interface[@name='{MANAGER}']")
    for method in ['AddProjectLabels', 'RemoveProjectLabels']:
        args = manager.find(f"method[@name='{method}']").findall('arg')
        assert [(a.get('type'), a.get('direction')) for a in args] == [('s', 'in'), ('as', 'in'), ('as', 'out')]
    for method, signature in [('ListProjects', 'a(ssssss)'), ('RenameProject', '(ssssss)'),
                              ('ResolveProject', '(ssssss)'), ('RegisterProject', '(ssssss)')]:
        out = manager.find(f"method[@name='{method}']/arg[@direction='out']")
        assert out.get('type') == signature, (method, out.attrib)
    xml = ET.fromstring(invoke(client, project_path, 'org.freedesktop.DBus.Introspectable', 'Introspect')[0])
    label_property = xml.find(f"interface[@name='{PROJECT}']/property[@name='Labels']")
    assert label_property.get('type') == 'as' and label_property.get('access') == 'read'

    received = []
    def signal(_conn, _sender, path, interface, member, parameters, _data):
        received.append((path, interface, member, parameters.unpack()))
    client.signal_subscribe(BUS, None, None, None, None, Gio.DBusSignalFlags.NONE, signal, None)
    # Round-trip on the subscription connection ensures AddMatch has reached the bus.
    invoke(client, PATH, 'org.freedesktop.DBus.Peer', 'Ping')
    expected = ['Bug', 'bug', 'team  name', 'équipe']
    assert labels('AddProjectLabels', [' bug ', 'Bug', 'bug', 'team  name', '\u2003équipe\u2003'], conn=client) == expected
    pump_until(lambda: any(e[2] == 'Changed' and e[3] == ('project', project_id) for e in received))
    changes = [e[3] for e in received if e[1] == PROPERTIES and e[3][0] == PROJECT]
    assert changes == [(PROJECT, {'Labels': expected}, [])], changes
    assert prop('Labels') == expected
    assert objects()[project_path][PROJECT]['Labels'] == expected

    received.clear()
    assert labels('AddProjectLabels', [' bug ', 'Bug'], conn=client) == expected
    assert labels('RemoveProjectLabels', ['absent'], conn=client) == expected
    for method in ['AddProjectLabels', 'RemoveProjectLabels']:
        assert labels(method, [], conn=client) == expected
        for invalid in ['', '   ', 'bad\n', '\tbad', 'a\u007fb', 'a\u0085b']:
            try:
                labels(method, ['Bug', 'valid-before-invalid', invalid], conn=client)
                raise AssertionError('accepted invalid batch')
            except GLib.Error as error:
                assert 'InvalidArgs' in str(error), error
        try:
            labels(method, [], id='unknown', conn=client)
            raise AssertionError('accepted unknown project')
        except GLib.Error as error:
            assert 'FileNotFound' in str(error), error
    assert prop('Labels') == expected
    # A distinct signal sent after the no-ops is a deterministic stream barrier.
    call('SetProjectIcon', '(ss)', (project_id, 'BARRIER'), client)
    pump_until(lambda: any(e[2] == 'Changed' and e[3] == ('project-icon', project_id) for e in received))
    assert not any(e[2] == 'Changed' and e[3] == ('project', project_id) for e in received), received
    assert not any(e[1] == PROPERTIES and 'Labels' in e[3][1] for e in received), received

    received.clear()
    assert labels('RemoveProjectLabels', ['Bug', 'team  name'], conn=client) == ['bug', 'équipe']
    pump_until(lambda: any(e[2] == 'Changed' and e[3] == ('project', project_id) for e in received))
    assert [e[3] for e in received if e[1] == PROPERTIES and e[3][0] == PROJECT] == [
        (PROJECT, {'Labels': ['bug', 'équipe']}, [])]
    labels('AddProjectLabels', expected)

    assert json.loads(cli('project', 'labels', 'get', '--json', cwd=directory)) == expected
    assert json.loads(cli('metadata', str(directory), '--json'))['labels'] == expected
    assert json.loads(cli('project', 'labels', 'remove', 'Bug', 'team  name', '--project', str(directory), '--json')) == ['bug', 'équipe']
    assert cli('project', 'labels', 'get', '--project-id', project_id) == 'bug\néquipe\n'
    assert json.loads(cli('project', 'labels', 'add', 'cli label', 'Bug', '--project-id', project_id, '--json')) == ['Bug', 'bug', 'cli label', 'équipe']
    fails('project', 'labels', 'add', '--project-id', project_id)
    fails('project', 'labels', 'get', '--project-id', project_id, '--project', str(directory))
    fails('project', 'labels', 'remove', 'Bug', ' ', '--project-id', project_id)
    unregistered = base / 'unregistered'
    unregistered.mkdir()
    for op, values in [('get', []), ('add', ['new']), ('remove', ['Bug'])]:
        fails('project', 'labels', op, *values, cwd=unregistered)
    assert len(json.loads(cli('project', 'list', '--json'))) == 1
    other = base / 'independent'
    other.mkdir()
    other_id = cli('add', str(other)).strip()
    assert labels('AddProjectLabels', ['Bug', 'independent'], id=other_id) == ['Bug', 'independent']

    # Different clients race batches against rename/icon and deletion.
    labels('AddProjectLabels', ['remove-me'])
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        jobs = [pool.submit(labels, 'AddProjectLabels', [f'race-{i:02}']) for i in range(24)]
        jobs += [pool.submit(labels, 'RemoveProjectLabels', ['remove-me']),
                 pool.submit(call, 'RenameProject', '(ss)', (project_id, 'Renamed')),
                 pool.submit(call, 'SetProjectIcon', '(ss)', (project_id, 'M'))]
        for job in jobs:
            job.result()
    expected = sorted(['Bug', 'bug', 'cli label', 'équipe'] + [f'race-{i:02}' for i in range(24)])
    assert prop('Labels') == expected
    assert prop('Name') == 'Renamed' and prop('Icon') == 'M'
    daemon.terminate()
    daemon.wait(timeout=5)
    daemon = subprocess.Popen([PROJD])
    ready()
    assert prop('Labels') == expected
    assert objects()[project_path][PROJECT]['Labels'] == expected
    assert json.loads(cli('metadata', str(directory), '--json'))['labels'] == expected
    assert labels('RemoveProjectLabels', expected) == []
    labels('AddProjectLabels', ['before-delete'])

    def racing_add(i):
        try:
            labels('AddProjectLabels', [f'late-{i}'])
        except GLib.Error as error:
            assert 'FileNotFound' in str(error), error
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        jobs = [pool.submit(racing_add, i) for i in range(24)]
        jobs.append(pool.submit(call, 'RemoveProject', '(s)', (project_id,)))
        for job in jobs:
            job.result()
    assert project_path not in objects()
    assert [p['id'] for p in json.loads(cli('project', 'list', '--json'))] == [other_id]
    assert json.loads(cli('project', 'labels', 'get', '--project-id', other_id, '--json')) == ['Bug', 'independent']
    with sqlite3.connect(os.environ['PROJD_STORE_PATH']) as db:
        assert db.execute("SELECT count(*) FROM records WHERE kind='project-labels' AND id=?", (project_id,)).fetchone()[0] == 0
    fails('project', 'labels', 'add', 'resurrect', '--project-id', project_id)
    new_id = cli('add', str(directory)).strip()
    assert new_id != project_id
    assert json.loads(cli('project', 'labels', 'get', '--project-id', new_id, '--json')) == []
    daemon.terminate()
    daemon.wait(timeout=5)
    daemon = subprocess.Popen([PROJD])
    ready()
    assert json.loads(cli('project', 'labels', 'get', '--project-id', new_id, '--json')) == []
    print('PASS: labels old-store default, unchanged DTO signatures, read-only property, atomic validation, '
          'signals/no-ops, CLI targeting/JSON, concurrent batches/metadata/deletion, restart and cleanup')
finally:
    daemon.terminate()
    daemon.wait(timeout=5)
