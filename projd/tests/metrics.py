"""Private-bus lifecycle/work metrics, ownership, alias reuse and crash coverage."""
import json, os, pathlib, sqlite3, subprocess, sys, tempfile, time
from gi.repository import Gio, GLib

ROOT=pathlib.Path(__file__).resolve().parents[2]
BIN=ROOT/'steward/target/release'
if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='projd-metrics-') as temp:
        env=dict(os.environ, PROJD_STORE_PATH=temp+'/store.sqlite3', TEST_ROOT=temp, XDG_DATA_HOME=temp, XDG_DATA_DIRS=temp)
        subprocess.run(['dbus-run-session','--',sys.executable,__file__,'--child'],env=env,check=True)
    sys.exit(0)

def cli(*args): return subprocess.check_output([str(BIN/'proj'),*args],text=True)
def connection():
    return Gio.DBusConnection.new_for_address_sync(os.environ['DBUS_SESSION_BUS_ADDRESS'],Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT|Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,None,None)
def call(conn,method,signature,args):
    return conn.call_sync('org.rsynapse.Proj','/org/rsynapse/Proj','org.rsynapse.Proj.Metrics1',method,GLib.Variant(signature,args),None,Gio.DBusCallFlags.NONE,5000,None).unpack()
def identities(id): return json.loads(cli('goal','instances',id,'--json'))
def stats(id,*args): return json.loads(cli('goal','stats',id,*args,'--json'))
def timeline(id,*args): return json.loads(cli('goal','timeline',id,*args,'--json'))
def ready():
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        result=subprocess.run([str(BIN/'proj'),'project','list','--json'],capture_output=True)
        if result.returncode==0: return
    raise AssertionError('daemon did not become ready')

base=pathlib.Path(os.environ['TEST_ROOT']); project=base/'project';project.mkdir()
legacy=dict(id='legacy',date='2026-10-07',title='Existing habit',kind='habit',project='',success='Criterion',priority='medium',status='in-progress')
db=sqlite3.connect(os.environ['PROJD_STORE_PATH']);db.execute('CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id))');db.execute('INSERT INTO records VALUES(?,?,?)',('goal','2026-10-07/legacy',json.dumps(legacy)));db.commit();db.close()
daemon=subprocess.Popen([str(BIN/'rsynapse-projd')])
try:
    ready()
    legacy_stats=stats('2026-10-07/legacy');assert legacy_stats['history_partial'] and legacy_stats['created_at_ms']==0 and legacy_stats['first_started_at_ms']==0
    cli('add',str(project),'--name','Metrics fixture')
    cli('goal','add','root','--project',str(project),'--title','Root goal')
    cli('goal','status','root','in-progress')
    cli('goal','add','child','--parent','root','--title','Work item')
    cli('goal','start','child')
    root_id=identities('root')[0]['instance_id']; child_id=identities('child')[0]['instance_id']
    assert identities('child')[0]['ancestor_ids']==[root_id]
    producer=connection()
    session=call(producer,'BeginGoalWork','(sss)',(child_id,'active','test.focus'))
    assert session[1]==child_id and session[6]==[root_id] and session[12], session
    # Real operations consume measurable monotonic time; correctness uses no sleep.
    cli('goal','update','child','--title','Renamed work item')
    live=timeline('root');assert len(live)==1 and live[0]['open'] and live[0]['duration_ms']>0
    try:
        call(connection(),'EndGoalWork','(ss)',(session[0],'wrong-owner'))
        raise AssertionError('another producer closed the work session')
    except GLib.Error as error: assert 'AccessDenied' in str(error)
    closed=call(producer,'EndGoalWork','(ss)',(session[0],'changed'))
    assert closed[11]>0 and not closed[12]
    assert stats('child')['active_ms']==stats('root')['active_ms']>0
    assert stats('root','--direct')['active_ms']==0
    history=json.loads(cli('goal','history','child','--json'))
    assert history[0]['goal']['title']=='Work item' and history[-1]['goal']['title']=='Renamed work item'
    idle=call(producer,'BeginGoalWork','(sss)',(child_id,'idle','test.focus'))
    cli('goal','show','child')
    call(producer,'EndGoalWork','(ss)',(idle[0],'resumed'))
    assert stats('child')['idle_ms']>0
    report=json.loads(cli('report','--all','--json'));assert len(report)==1 and report[0]['project']=='Metrics fixture' and report[0]['active_ms']>0
    cli('goal','complete','child');cli('goal','start','child');cli('goal','complete','child')
    s=stats('child');assert s['completion_count']==2 and s['first_started_at_ms']>0 and s['completed_at_ms']>0 and not s['history_partial']
    cli('goal','remove','child')
    assert stats(child_id)['active_ms']>0 and identities('child')[0]['deleted_at_ms']>0
    cli('goal','add','child','--parent','root','--title','A different task')
    new_id=next(i['instance_id'] for i in identities('child') if i['deleted_at_ms']==0)
    assert new_id!=child_id and stats('child')['active_ms']==0
    assert stats('root')['active_ms']==s['active_ms']
    # Losing a persistent producer connection closes its session on NameOwnerChanged.
    disposable=connection();lost=call(disposable,'BeginGoalWork','(sss)',(new_id,'active','test.disconnect'))
    disposable.close_sync(None)
    deadline=time.monotonic()+5
    while time.monotonic()<deadline:
        info=call(producer,'GetWorkSession','(s)',(lost[0],))
        if not info[12]:break
        GLib.MainContext.default().iteration(False)
    assert not info[12] and info[13]=='source-disconnected'
    # A recorder crash cannot recover elapsed time from an unclosed row: mark a gap.
    crash=call(producer,'BeginGoalWork','(sss)',(new_id,'active','test.crash'))
    daemon.kill();daemon.wait(timeout=5);daemon=subprocess.Popen([str(BIN/'rsynapse-projd')]);ready()
    interrupted=next(w for w in timeline(new_id) if w['id']==crash[0])
    assert not interrupted['open'] and interrupted['duration_ms']==0 and 'unknown-duration' in interrupted['end_reason']
    assert stats(new_id)['history_partial'] and stats(new_id)['interrupted_sessions']==1
    assert stats(child_id)['active_ms']==s['active_ms']
    print('PASS: lifecycle events, legacy unknown times, open/closed active+idle spans, parent rollup, ownership, alias reuse, queries, disconnect closure and crash coverage')
finally:
    daemon.terminate();daemon.wait(timeout=5)
