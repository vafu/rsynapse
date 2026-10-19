"""HTTP/SSE -> real Metrics1 on a private bus; no production state or UI actions."""
import json,os,pathlib,socket,subprocess,sys,tempfile,time,urllib.error,urllib.request
from gi.repository import Gio,GLib
ROOT=pathlib.Path(__file__).resolve().parents[2];BIN=ROOT/'steward/target/release'
if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='goal-dashboard-metrics-') as temp:
        env=dict(os.environ,TEST_ROOT=temp,PROJD_STORE_PATH=temp+'/store.sqlite3',LOCUS_RELATIONS_PATH=temp+'/relations.json',XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp)
        subprocess.run(['dbus-run-session','--',sys.executable,__file__,'--child'],env=env,check=True)
    sys.exit(0)
with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
url=f'http://127.0.0.1:{port}'
def cli(*args):return subprocess.check_output([str(BIN/'proj'),*args],text=True)
def get(path,status=200,method='GET'):
    req=urllib.request.Request(url+path,method=method,headers={'Origin':'http://localhost:3000'})
    try:response=urllib.request.urlopen(req,timeout=10)
    except urllib.error.HTTPError as e:response=e
    with response:
        body=response.read();assert response.status==status,(path,response.status,body)
        assert response.headers.get('Access-Control-Allow-Origin')=='http://localhost:3000'
        return json.loads(body) if response.status==200 else body
def wait(fn):
    end=time.monotonic()+10
    while time.monotonic()<end:
        try:
            result=fn()
            if result:return result
        except Exception:pass
    raise AssertionError('Private service was not ready')
conn=Gio.bus_get_sync(Gio.BusType.SESSION,None)
def metrics(method,sig,args):return conn.call_sync('org.rsynapse.Proj','/org/rsynapse/Proj','org.rsynapse.Proj.Metrics1',method,GLib.Variant(sig,args),None,Gio.DBusCallFlags.NONE,5000,None).unpack()
processes=[subprocess.Popen([str(pathlib.Path.home()/'.local/bin/rsynapse-locusd')]),subprocess.Popen([str(BIN/'rsynapse-projd')])]
try:
    wait(lambda:subprocess.run([str(BIN/'proj'),'project','list','--json'],capture_output=True).returncode==0)
    base=pathlib.Path(os.environ['TEST_ROOT']);first=base/'first';second=base/'second';first.mkdir();second.mkdir()
    project=cli('add',str(first),'--name','Project A').strip();other=cli('add',str(second),'--name','Project B').strip()
    cli('goal','add','root','--project',str(first),'--title','Objective');cli('goal','status','root','in-progress')
    cli('goal','add','child','--parent','root','--title','Concrete step');cli('goal','start','child')
    identities=json.loads(cli('goal','instances','--json'));root=next(g['instance_id'] for g in identities if g['key']=='root');child=next(g['instance_id'] for g in identities if g['key']=='child')
    processes.append(subprocess.Popen([str(BIN/'rsynapse-dashboard-api')],env=dict(os.environ,RSYNAPSE_DASHBOARD_PORT=str(port))))
    wait(lambda:get('/health'))
    stream=urllib.request.urlopen(url+'/api/events',timeout=5)
    assert b'connected' in stream.readline();stream.readline();stream.readline()
    session=metrics('BeginGoalWork','(sss)',(child,'active','test.dashboard'))
    while True:
        event=stream.readline();assert event,'SSE stream ended'
        if b'goal-metrics-changed' in event:break
    stream.close()
    cli('goal','show','child')
    view=get('/api/goal-metrics?from_ms=0')
    assert len(view['work'])==1 and view['work'][0]['open']
    rows={g['instance_id']:g for g in view['stats']}
    assert rows[root]['active_ms']==rows[child]['active_ms']==view['work'][0]['duration_ms']>0,(rows,view['work'])
    assert len([w for w in view['work'] if w['kind']=='active'])==1
    scoped=get(f'/api/goal-metrics?instance_id={root}&include_subgoals=false')
    assert len(scoped['stats'])==1 and scoped['stats'][0]['active_ms']==0 and scoped['work']==[]
    assert get(f'/api/goal-metrics?project_id={other}')['work']==[]
    assert len(get(f'/api/goal-metrics?project_id={project}')['projects'])>=1
    metrics('EndGoalWork','(ss)',(session[0],'finished'))
    cli('goal','complete','child');cli('goal','remove','child');cli('goal','add','child','--parent','root','--title','New lifetime')
    old=get(f'/api/goal-metrics?instance_id={child}')
    assert len(old['stats'])==1 and old['stats'][0]['active_ms']>0 and any(e['kind']=='deleted' for e in old['history'])
    assert next(g for g in old['identities'] if g['instance_id']==child)['deleted_at_ms']>0
    get('/api/goal-metrics?from_ms=100&to_ms=1',400)
    get('/api/goal-metrics?from_ms=invalid',400)
    get('/api/goal-metrics?instance_id=unknown',404)
    get('/api/goal-metrics',405,method='POST')
    print('PASS: metrics HTTP projection, open spans, consistent parent/child effort, project/date/task filtering, removed identities, typed errors, read-only routes and WorkChanged SSE')
finally:
    for process in reversed(processes):process.terminate();process.wait(timeout=5)
