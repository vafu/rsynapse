"""Exercise migration against private real services, including conflict safety."""
import json,os,pathlib,subprocess,sys,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[2]
PROJ=os.environ.get("PROJ_BIN",str(ROOT/"steward/target/release/proj"))
if "--child" not in sys.argv:
    with tempfile.TemporaryDirectory(prefix="projd-migration-") as temp:
        env=dict(os.environ,PROJ_BIN=PROJ,LOCUS_RELATIONS_PATH=temp+"/relations.json",PROJD_STORE_PATH=temp+"/projd.sqlite3",XDG_STATE_HOME=temp,XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp)
        subprocess.run(["dbus-run-session","--",sys.executable,__file__,"--child"],env=env,check=True)
    sys.exit(0)
def call(method,*args):
    return json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1",method,*args],text=True))["data"][0]
def endpoint(kind,id):return ["3","type","stable-key","kind",kind,"id",id]
def bind(kind,id,relation,target_kind,target,metadata,persist):
    flat=[str(len(metadata))]
    for k,v in metadata.items():flat.extend([k,v])
    call("SetWithPersistence","a{ss}sa{ss}a{ss}b",*endpoint(kind,id),relation,*endpoint(target_kind,target),*flat,str(persist).lower())
daemons=[subprocess.Popen([os.environ["LOCUS_BIN"]]),subprocess.Popen([str(ROOT/"steward/target/release/projd")])]
try:
    for _ in range(500):
        try:call("List","s","");subprocess.check_output([PROJ,"goal","list","--all","--json"],stderr=subprocess.DEVNULL);break
        except subprocess.CalledProcessError:time.sleep(.02)
    path=os.environ["XDG_STATE_HOME"]
    relation="org.rsynapse.workspace.project"
    for id,persist in [("1",True),("2",False)]:
        bind("org.rsynapse.niri.workspace.id",id,relation,"org.rsynapse.project.path",path,{"cwd-path":path},persist)
    bind("org.rsynapse.niri.workspace.id","3",relation,"org.rsynapse.project.path",path+"/missing",{},True)
    goals=[dict(id=id,date="2026-10-07",kind="habit",title="Saved "+id,project=None,success="Keep criterion "+id,priority="high",status="planned") for id in ["first","second"]]
    for goal in goals:
        bind("org.rsynapse.calendar-day",goal["date"],"org.rsynapse.day.goal","org.rsynapse.goal",goal["date"]+"/"+goal["id"],{"goal":json.dumps(goal)},True)
    def migrate(**kw):return subprocess.run([sys.executable,str(ROOT/"install/migrate-projd.py")],**kw)
    migrate(check=True)
    assert json.loads(subprocess.check_output([PROJ,"goal","list","--all","--json"],text=True))==goals
    states=call("ListWithPersistence","s",relation)
    assert {r[0]["id"]:p for r,p in states}=={"1":True,"2":False,"3":True}
    assert all("context-id" in r[3] for r,p in states if r[0]["id"]!="3")
    assert len(call("List","s","org.rsynapse.day.goal"))==2
    migrate(check=True)
    subprocess.run([PROJ,"goal","status","first","completed","--date","2026-10-07"],check=True)
    bind("org.rsynapse.calendar-day","2026-10-07","org.rsynapse.day.goal","org.rsynapse.goal","2026-10-07/first",{"goal":json.dumps(goals[0])},True)
    result=migrate(capture_output=True,text=True)
    assert result.returncode!=0 and "Imported goal content differs" in result.stderr
    assert any("goal" in r[3] for r in call("List","s","org.rsynapse.day.goal"))
    print("PASS: full goal content, multiple goals/day, existing persistence, missing paths, repeat import and conflict retention")
finally:
    for daemon in daemons:daemon.terminate();daemon.wait(timeout=5)
