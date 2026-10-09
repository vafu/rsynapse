"""Legacy grouped worktrees become distinct projects; contexts disappear atomically."""
import json,os,pathlib,sqlite3,subprocess,sys,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[2];PROJD=str(ROOT/'steward/target/release/projd');PROJ=str(ROOT/'steward/target/release/proj')
if '--child' not in sys.argv:
    with tempfile.TemporaryDirectory(prefix='projd-model-migrate-') as temp:
        base=pathlib.Path(temp);repo=base/'repo';repo.mkdir()
        subprocess.run(['git','-C',str(repo),'init','-b','main'],check=True,stdout=subprocess.DEVNULL)
        (repo/'keep').write_text('unchanged');subprocess.run(['git','-C',str(repo),'add','keep'],check=True)
        subprocess.run(['git','-C',str(repo),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-m','Fixture'],check=True,stdout=subprocess.DEVNULL)
        wt=base/'feature';subprocess.run(['git','-C',str(repo),'worktree','add','-b','feature',str(wt)],check=True,stdout=subprocess.DEVNULL)
        store=base/'store.sqlite3';db=sqlite3.connect(store);db.execute('CREATE TABLE records(kind TEXT,id TEXT,json TEXT,PRIMARY KEY(kind,id))')
        status=dict(staged=0,unstaged=0,untracked=0,ahead=0,behind=0,merging=False,rebasing=False,stashes=0)
        entries=[('project','p',dict(id='p',name='Custom primary',root_path=str(repo))),
            ('checkout','a',dict(id='a',project_id='p',root_path=str(repo),branch='main',git_status=status)),
            ('checkout','b',dict(id='b',project_id='p',root_path=str(wt),branch='feature',git_status=status)),
            ('context','ctx',dict(id='ctx',project_id='p',checkout_id='b',cwd=str(wt/'subdir'),relative_cwd='subdir'))]
        for kind,id,data in entries:db.execute('INSERT INTO records VALUES(?,?,?)',(kind,id,json.dumps(data)))
        db.commit();db.close()
        env=dict(os.environ,PROJD_STORE_PATH=str(store),TEST_ROOT=temp,XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp)
        subprocess.run(['dbus-run-session','--',sys.executable,__file__,'--child'],env=env,check=True)
    sys.exit(0)
daemon=subprocess.Popen([PROJD])
def cli(*args):return json.loads(subprocess.check_output([PROJ,*args],text=True))
try:
    for _ in range(500):
        try:
            projects=cli('project','list','--json')
            if projects:break
        except Exception:pass
        time.sleep(.02)
    assert len(projects)==2
    p=next(p for p in projects if p['checkout_id']=='a');assert p['id']=='p' and p['name']=='Custom primary'
    wt=next(p for p in projects if p['checkout_id']=='b');assert wt['id']!='p' and wt['name']=='feature'
    db=sqlite3.connect(os.environ['PROJD_STORE_PATH']);assert db.execute("SELECT count(*) FROM records WHERE kind='context'").fetchone()[0]==0;db.close()
    assert list(pathlib.Path(os.environ['TEST_ROOT']).glob('before-model-v2-*.sqlite3'))
    assert (pathlib.Path(os.environ['TEST_ROOT'])/'repo/keep').read_text()=='unchanged'
    objects=json.loads(subprocess.check_output(['busctl','--user','--json=short','call','org.rsynapse.Proj','/org/rsynapse/Proj','org.freedesktop.DBus.ObjectManager','GetManagedObjects'],text=True))['data'][0]
    assert '/org/rsynapse/Proj/Projects/pp' in objects and '/org/rsynapse/Proj/Checkouts/ca' in objects
    assert not any('Context1' in str(i) for i in objects.values())
    print('PASS: metadata migration splits checkout projects, preserves custom primary name/IDs, removes contexts, creates a backup and leaves files untouched')
finally:daemon.terminate();daemon.wait(timeout=5)
