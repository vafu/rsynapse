"""Registration and CRUD remain responsive during a deliberately blocked Git scan."""
import json,os,pathlib,shutil,signal,subprocess,sys,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[2]
PROJ=str(ROOT/'steward/target/release/proj');PROJD=str(ROOT/'steward/target/release/projd')
if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='projd-responsive-') as temp:
        base=pathlib.Path(temp);bin=base/'bin';bin.mkdir()
        wrapper=bin/'git'
        wrapper.write_text('''#!/usr/bin/env python3
import os,pathlib,signal,sys,threading
base=pathlib.Path(os.environ['TEST_PROJECT_DIR'])
if 'status' in sys.argv and sys.argv[1:3]==['-C',str(base/'repo')] and (base/'block').exists():
    ready=threading.Event()
    signal.signal(signal.SIGUSR1,lambda *_:ready.set())
    (base/'blocked-pid').write_text(str(os.getpid()))
    ready.wait()
os.execv(os.environ['TEST_REAL_GIT'],[os.environ['TEST_REAL_GIT'],*sys.argv[1:]])
''')
        wrapper.chmod(0o755)
        env=dict(os.environ,PATH=str(bin)+':'+os.environ['PATH'],TEST_REAL_GIT=shutil.which('git'),TEST_PROJECT_DIR=temp,PROJD_STORE_PATH=temp+'/store.sqlite3',XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp)
        subprocess.run(['dbus-run-session','--',sys.executable,__file__,'--child'],env=env,check=True)
    sys.exit(0)
base=pathlib.Path(os.environ['TEST_PROJECT_DIR']);repo=base/'repo';repo.mkdir()
subprocess.run(['git','-C',str(repo),'init','-b','main'],check=True,stdout=subprocess.DEVNULL)
daemon=subprocess.Popen([PROJD]);refresh=None
def cli(*args):return subprocess.check_output([PROJ,*args],text=True,timeout=3)
def wait(fn):
    until=time.monotonic()+10
    while time.monotonic()<until:
        try:
            value=fn()
            if value:return value
        except Exception:pass
        time.sleep(.02)
    raise AssertionError('Condition not observed')
try:
    wait(lambda:cli('project','list','--json'))
    cli('add',str(repo))
    (base/'block').touch()
    refresh=subprocess.Popen([PROJ,'refresh',str(repo)],stdout=subprocess.DEVNULL)
    wait(lambda:(base/'blocked-pid').exists())
    child=repo/'nested';child.mkdir()
    registered=json.loads(cli('metadata',str(child),'--json'))
    assert registered['relative-cwd']=='nested'
    cli('goal','add','during-refresh','--title','Responsive CRUD','--project',str(repo),'--success','Git scan does not hold domain mutations','--date','2026-10-08')
    assert json.loads(cli('goal','list','--all','--json'))[0]['id']=='during-refresh'
    (base/'block').unlink();os.kill(int((base/'blocked-pid').read_text()),signal.SIGUSR1)
    assert refresh.wait(timeout=10)==0
    print('PASS: context registration and goal CRUD complete while a checkout Git scan is blocked')
finally:
    if (base/'block').exists(): (base/'block').unlink()
    if (base/'blocked-pid').exists():
        try:os.kill(int((base/'blocked-pid').read_text()),signal.SIGUSR1)
        except ProcessLookupError:pass
    if refresh is not None and refresh.poll() is None:refresh.terminate();refresh.wait(timeout=5)
    daemon.terminate();daemon.wait(timeout=5)
