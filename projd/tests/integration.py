"""Real Project/Checkout object references, independent worktrees, and durable icons."""
import concurrent.futures,json,os,pathlib,subprocess,sys,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[2]
PROJD=os.environ.get('PROJD_BIN',str(ROOT/'steward/target/release/projd'));PROJ=os.environ.get('PROJ_BIN',str(ROOT/'steward/target/release/proj'))
if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='projd-test-') as temp:
        env=dict(os.environ,PROJD_STORE_PATH=temp+'/store.sqlite3',XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp,TEST_PROJECT_DIR=temp)
        subprocess.run(['dbus-run-session','--',sys.executable,__file__,'--child'],env=env,check=True)
    sys.exit(0)
def cli(*args):return subprocess.check_output([PROJ,*args],text=True)
def call(method,sig=None,*args):
    cmd=['busctl','--user','--json=short','call','org.rsynapse.Proj','/org/rsynapse/Proj','org.rsynapse.Proj.Manager1',method]
    if sig:cmd.extend([sig,*args])
    return json.loads(subprocess.check_output(cmd,text=True))['data']
def prop(path,interface,name):
    data=json.loads(subprocess.check_output(['busctl','--user','--json=short','get-property','org.rsynapse.Proj',path,interface,name],text=True))['data']
    return data if isinstance(data,str) else data[0]
def wait(fn):
    until=time.monotonic()+10
    while time.monotonic()<until:
        try:
            result=fn()
            if result:return result
        except Exception:pass
        time.sleep(.02)
    raise AssertionError('condition not observed')
temp=pathlib.Path(os.environ['TEST_PROJECT_DIR']);repo=temp/'repo';repo.mkdir()
def git(*args):subprocess.run(['git','-C',str(repo),*args],check=True,stdout=subprocess.DEVNULL)
git('init','-b','main');(repo/'file.txt').write_text('one\n');git('add','file.txt');git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-m','Initial')
daemon=subprocess.Popen([PROJD])
try:
    wait(lambda:cli('project','list','--json'))
    project=cli('add',str(repo),'--name','Fixture project').strip()
    first=json.loads(cli('metadata',str(repo),'--json'));assert first['id']==project and first['branch']=='main'
    alternate=temp/'alternate';git('worktree','add','-b','alternate',str(alternate))
    other_project=cli('add',str(alternate)).strip();assert other_project!=project
    other=json.loads(cli('metadata',str(alternate),'--json'));assert other['name']=='alternate' and other['checkout-id']!=first['checkout-id']
    child=repo/'android/snapchat';child.mkdir(parents=True)
    nested=json.loads(cli('metadata',str(child),'--json'));assert nested['id']==project and nested['name']=='Fixture project' and nested['cwd']==str(repo)
    assert 'context-id' not in nested and 'relative-cwd' not in nested
    cli('add',str(child));assert json.loads(cli('metadata',str(repo),'--json'))['cwd']==str(child)
    ppath='/org/rsynapse/Proj/Projects/p'+project;cpath='/org/rsynapse/Proj/Checkouts/c'+first['checkout-id']
    assert prop(ppath,'org.rsynapse.Proj.Project1','Checkout')==cpath,(prop(ppath,'org.rsynapse.Proj.Project1','Checkout'),cpath)
    assert prop(cpath,'org.rsynapse.Proj.Checkout1','Project')==ppath
    assert prop(ppath,'org.rsynapse.Proj.Project1','Cwd')==str(child)
    plain=temp/'plain';plain.mkdir();plain_id=cli('add',str(plain)).strip()
    assert prop('/org/rsynapse/Proj/Projects/p'+plain_id,'org.rsynapse.Proj.Project1','Checkout')=='/'
    assert json.loads(cli('metadata',str(plain),'--json'))['checkout'] is None
    call('SetProjectIconIfUnset','ss',project,'A');assert prop(ppath,'org.rsynapse.Proj.Project1','Icon')=='A'
    call('SetProjectIcon','ss',project,'M');call('SetProjectIconIfUnset','ss',project,'LATE')
    assert prop(ppath,'org.rsynapse.Proj.Project1','Icon')=='M' and prop(ppath,'org.rsynapse.Proj.Project1','IconOrigin')=='manual'
    call('ClearProjectIcon','s',project)
    with concurrent.futures.ThreadPoolExecutor(2) as ex:
        futures=[ex.submit(call,'SetProjectIconIfUnset','ss',project,'AUTO'),ex.submit(call,'SetProjectIcon','ss',project,'MANUAL')]
        [f.result() for f in futures]
    assert prop(ppath,'org.rsynapse.Proj.Project1','Icon')=='MANUAL'
    cli('goal','add','habit1','--kind','habit','--title','Personal budget','--success','At most 30 minutes','--date','2026-10-07')
    cli('goal','add','outcome1','--project',str(repo),'--title','Preserved outcome','--success','Preserved criterion','--date','2026-10-07')
    assert prop(ppath,'org.rsynapse.Proj.Project1','Cwd')==str(child)
    goals=json.loads(cli('goal','list','--all','--json'))
    git('checkout','-b','new-branch')
    wait(lambda:any(c['branch']=='new-branch' for c in json.loads(cli('checkout','list','--json'))))
    objects=json.loads(subprocess.check_output(['busctl','--user','--json=short','call','org.rsynapse.Proj','/org/rsynapse/Proj','org.freedesktop.DBus.ObjectManager','GetManagedObjects'],text=True))['data'][0]
    assert not any('Context1' in str(i) for i in objects.values())
    daemon.terminate();daemon.wait(timeout=5);daemon=subprocess.Popen([PROJD]);wait(lambda:cli('project','list','--json'))
    assert prop(ppath,'org.rsynapse.Proj.Project1','Icon')=='MANUAL'
    original=(repo/'file.txt').read_bytes();mtime=(repo/'file.txt').stat().st_mtime_ns
    cli('remove',project)
    assert not any(p['id']==project for p in json.loads(cli('project','list','--json')))
    assert any(p['id']==other_project for p in json.loads(cli('project','list','--json')))
    assert json.loads(cli('goal','list','--all','--json'))==goals
    assert (repo/'file.txt').read_bytes()==original and (repo/'file.txt').stat().st_mtime_ns==mtime
    assert subprocess.run([PROJ,'metadata',str(repo)],capture_output=True).returncode!=0
    cli('goal','status','outcome1','completed','--date','2026-10-07')
    assert not any(p['id']==project or p['checkout_id']==first['checkout-id'] for p in json.loads(cli('project','list','--json')))
    assert subprocess.run([PROJ,'metadata',str(repo)],capture_output=True).returncode!=0
    assert cli('add',str(repo)).strip()!=project
    print('PASS: distinct checkout projects, optional Checkout/reverse Project object references, project-owned CWD, no Context objects, durable icons, manual/auto race protection, Git streams and metadata-only removal')
finally:daemon.terminate();daemon.wait(timeout=5)
