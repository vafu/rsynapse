"""Persistent tasks, nested object references, notes, scoped active selection and races."""
import concurrent.futures
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / 'steward/target/release'
if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='projd-checklist-') as temp:
        env = dict(os.environ, PROJD_STORE_PATH=temp+'/store.sqlite3', PROJD_GOALS_DIR=temp+'/notes with spaces', TEST_ROOT=temp, XDG_DATA_HOME=temp, XDG_DATA_DIRS=temp)
        subprocess.run(['dbus-run-session', '--', sys.executable, __file__, '--child'], env=env, check=True)
    sys.exit(0)

def cli(*args, check=True):
    return subprocess.run([str(BIN/'proj'), *args], text=True, capture_output=True, check=check)
def call(method, signature, *args):
    return json.loads(subprocess.check_output(['busctl','--user','--json=short','call','org.rsynapse.Proj','/org/rsynapse/Proj','org.rsynapse.Proj.Manager1',method,signature,*args], text=True))['data']
def prop(path, interface, name):
    return json.loads(subprocess.check_output(['busctl','--user','--json=short','get-property','org.rsynapse.Proj',path,interface,name], text=True))['data']
def show(id): return json.loads(cli('goal','show',id).stdout)
def ready():
    deadline = time.monotonic()+10
    while time.monotonic()<deadline:
        if cli('project','list','--json',check=False).returncode==0: return
        time.sleep(.02)
    raise AssertionError('daemon did not become ready')

base = pathlib.Path(os.environ['TEST_ROOT'])
first = base/'project'; first.mkdir()
other = base/'other'; other.mkdir()
legacy=dict(id='archived',date='2026-10-07',title='Historical habit',kind='habit',project='',success='Keep complete criterion',priority='high',status='planned')
db=sqlite3.connect(os.environ['PROJD_STORE_PATH'])
db.execute('CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id))')
db.execute('INSERT INTO records VALUES(?,?,?)',('goal','2026-10-07/archived',json.dumps(legacy)))
db.commit();db.close()
daemon = subprocess.Popen([str(BIN/'rsynapse-projd')])
try:
    ready()
    archived=show('2026-10-07/archived')
    assert all(archived[key]==(None if key=='project' else value) for key,value in legacy.items())
    assert 'Keep complete criterion' in pathlib.Path(archived['note_path']).read_text()
    assert list(base.glob('before-goal-checklists-*.sqlite3'))
    project = cli('add',str(first)).stdout.strip()
    other_id = cli('add',str(other)).stdout.strip()
    top = cli('goal','add','launch','--project',str(first),'--title','Launch checklist','--description','Release the app','--active').stdout.strip()
    child = cli('goal','add','build','--parent',top,'--title','Build artifact').stdout.strip()
    grandchild = cli('goal','add','verify','--parent',child,'--title','Verify checksum').stdout.strip()
    assert show(top)['date']=='' and show(child)['parent_id']==top and show(grandchild)['parent_id']==child
    assert len(json.loads(cli('goal','list','--project',str(first),'--json').stdout))==3
    assert json.loads(cli('goal','list','--project',str(other),'--json').stdout)==[]
    top_path='/org/rsynapse/Proj/Goals/g'+top.encode().hex()
    child_path='/org/rsynapse/Proj/Goals/g'+child.encode().hex()
    assert prop(child_path,'org.rsynapse.Proj.Goal1','Parent')==top_path
    assert prop(top_path,'org.rsynapse.Proj.Goal1','Children')==[child_path]
    assert prop(top_path,'org.rsynapse.Proj.Goal1','Project')=='/org/rsynapse/Proj/Projects/p'+project
    assert prop('/org/rsynapse/Proj/Projects/p'+project,'org.rsynapse.Proj.Project1','ActiveGoal')==top_path
    assert subprocess.run(['busctl','--user','call','org.rsynapse.Proj','/org/rsynapse/Proj','org.rsynapse.Proj.Manager1','SetActiveGoal','ss',other_id,top],capture_output=True).returncode!=0
    note=pathlib.Path(cli('goal','note',child).stdout.strip())
    assert note.is_file() and '# Build artifact' in note.read_text()
    note.write_text(note.read_text()+'\nAgent notes: keep this exactly.\n')
    content=note.read_bytes()
    editor=base/'editor fixture.py'
    editor.write_text('import json,pathlib,sys\npathlib.Path(sys.argv[1]).write_text(json.dumps(sys.argv[2:]))\n')
    log=base/'editor-args.json'
    env=dict(os.environ, EDITOR=f"{sys.executable} '{editor}' '{log}'")
    subprocess.run([str(BIN/'proj'),'goal','note',child,'--open'],env=env,check=True)
    assert json.loads(log.read_text())==[str(note)]
    cli('goal','update',child,'--title','Build release','--description','Signed artifact')
    cli('goal','set',child,'--title','Build release v2')
    assert show(child)['description']=='Signed artifact' and note.read_bytes()==content
    cli('goal','start',child)
    assert show(child)['status']=='in-progress'
    assert json.loads(cli('goal','active','--project',str(first)).stdout)['id']==child
    with concurrent.futures.ThreadPoolExecutor(2) as executor:
        futures=[executor.submit(cli,'goal','update',child,'--description','Concurrent edit'), executor.submit(cli,'goal','complete',child)]
        [future.result() for future in futures]
    assert show(child)['description']=='Concurrent edit' and show(child)['status']=='completed'
    assert json.loads(cli('goal','active','--project',str(first)).stdout) is None
    assert show(top)['status']=='planned' and show(grandchild)['status']=='planned'
    assert cli('goal','active',child,check=False).returncode!=0
    assert cli('goal','remove',top,check=False).returncode!=0
    cli('goal','status',child,'todo')
    cli('goal','active',top)
    daemon.terminate(); daemon.wait(timeout=5)
    daemon=subprocess.Popen([str(BIN/'rsynapse-projd')]); ready()
    assert json.loads(cli('goal','active','--project',str(first)).stdout)['id']==top
    assert note.read_bytes()==content
    cli('goal','add','habit','--kind','habit','--title','Dated habit','--success','Preserve history','--date','2026-10-07')
    cli('goal','add','2026-10-07_habit','--global','--title','Persistent task with similar ID')
    assert cli('goal','note','2026-10-07/habit').stdout!=cli('goal','note','2026-10-07_habit').stdout
    cli('goal','remove',grandchild); cli('goal','remove',child)
    assert note.read_bytes()==content and prop(top_path,'org.rsynapse.Proj.Goal1','Children')==[]
    cli('remove',project)
    assert show(top)['title']=='Launch checklist'
    print('PASS: persistent hierarchy, per-project active selection, D-Bus references, notes, concurrent field/status changes, completion independence and restart persistence')
finally:
    daemon.terminate(); daemon.wait(timeout=5)
