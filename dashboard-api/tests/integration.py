"""Real HTTP -> session D-Bus -> locus integration in an isolated test bus."""
import concurrent.futures, json, os, pathlib, socket, subprocess, sys, tempfile, time, urllib.error, urllib.request
ROOT=pathlib.Path(__file__).resolve().parents[2]
API=os.environ.get("RSYNAPSE_API_BIN",str(ROOT/"steward/target/release/rsynapse-dashboard-api"))
LOCUS=os.environ.get("LOCUS_BIN",str(pathlib.Path.home()/".local/bin/locusd"))
PROJD=os.environ.get("PROJD_BIN",str(ROOT/"steward/target/release/projd"))
STEWARD=os.environ.get("STEWARD_BIN",str(ROOT/"steward/target/release/steward"))

if "--child" not in sys.argv:
    with tempfile.TemporaryDirectory(prefix="rsynapse-goal-api-") as temp:
        env=dict(os.environ,LOCUS_RELATIONS_PATH=temp+"/relations.json",PROJD_STORE_PATH=temp+"/projd.sqlite3",XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp,RSYNAPSE_TEST_DIR=temp)
        subprocess.run(["dbus-run-session","--",sys.executable,__file__,"--child"],env=env,check=True)
    sys.exit(0)

with socket.socket() as sock:
    sock.bind(("127.0.0.1",0));port=sock.getsockname()[1]
base=f"http://127.0.0.1:{port}"
env=dict(os.environ,RSYNAPSE_DASHBOARD_PORT=str(port))
locus=subprocess.Popen([LOCUS],env=env)
projd=subprocess.Popen([PROJD],env=env)
steward=None
api=None
def wait(fn):
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        try:
            value=fn()
            if value:return value
        except Exception:pass
        time.sleep(.02)
    raise AssertionError("Test service did not become ready")
def call(path,method="GET",body=None,expected=200,origin="http://localhost:3000"):
    headers={"Origin":origin}
    if body is not None:headers["Content-Type"]="application/json"
    req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers=headers,method=method)
    try:response=urllib.request.urlopen(req,timeout=5)
    except urllib.error.HTTPError as e:response=e
    with response:
        assert response.status==expected,(path,response.status,response.read())
        assert response.headers.get("Access-Control-Allow-Origin")==origin
        raw=response.read()
        return json.loads(raw) if raw else None
try:
    wait(lambda:subprocess.run(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","List","s","org.rsynapse.day.goal"],capture_output=True).returncode==0)
    wait(lambda:subprocess.run(["busctl","--user","--json=short","call","org.rsynapse.Proj","/org/rsynapse/Proj","org.rsynapse.Proj.Manager1","ListProjects"],capture_output=True).returncode==0)
    steward=subprocess.Popen([STEWARD,"associations"],env=env)
    wait(lambda:subprocess.run(["busctl","--user","status","org.rsynapse.Steward"],capture_output=True).returncode==0)
    def init_workspace(workspace,path,check=True):
        return subprocess.run(["busctl","--user","--json=short","call","org.rsynapse.Steward","/org/rsynapse/Steward","org.rsynapse.Steward.Associations1","InitWorkspaceProject","ts",str(workspace),path],capture_output=True,text=True,check=check)
    context=json.loads(init_workspace(77,os.environ["RSYNAPSE_TEST_DIR"]).stdout)["data"]
    bindings=json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","ListWithPersistence","s","org.rsynapse.workspace.project"],text=True))["data"][0]
    assert len(bindings)==1 and bindings[0][0][0]["id"]=="77" and bindings[0][1] is False
    assert bindings[0][0][3]["context-id"]==context[0] and bindings[0][0][3]["project-id"]==context[1]
    other=pathlib.Path(os.environ["RSYNAPSE_TEST_DIR"])/"other";other.mkdir()
    before_projects=subprocess.check_output([str(ROOT/"steward/target/release/proj"),"project","list","--json"])
    blocked=init_workspace(77,str(other),check=False)
    assert blocked.returncode!=0 and "already has a project" in blocked.stderr
    assert subprocess.check_output([str(ROOT/"steward/target/release/proj"),"project","list","--json"])==before_projects
    unchanged=json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","ListWithPersistence","s","org.rsynapse.workspace.project"],text=True))["data"][0]
    assert unchanged==bindings
    rebound=subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Steward","/org/rsynapse/Steward","org.rsynapse.Steward.Associations1","BindWorkspaceProject","ts","77",str(other)],text=True)
    new_context=json.loads(rebound)["data"]
    changed=json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","ListWithPersistence","s","org.rsynapse.workspace.project"],text=True))["data"][0]
    assert len(changed)==1 and changed[0][0][0]["id"]=="77" and changed[0][0][2]["id"]==str(other)
    assert changed[0][0][3]["context-id"]==new_context[0] and changed[0][1] is False
    subprocess.run(["busctl","--user","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","SetOneWithPersistence","a{ss}sa{ss}a{ss}b","3","type","stable-key","kind","org.rsynapse.niri.workspace.id","id","77","org.rsynapse.workspace.name","3","type","stable-key","kind","org.rsynapse.workspace.name","id","office","1","source","manual","true"],check=True,stdout=subprocess.DEVNULL)
    def relation_rows(relation):
        return json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","ListWithPersistence","s",relation],text=True))["data"][0]
    names=relation_rows("org.rsynapse.workspace.name")
    init_workspace(78,os.environ["RSYNAPSE_TEST_DIR"])
    projects=subprocess.check_output([str(ROOT/"steward/target/release/proj"),"project","list","--json"])
    subprocess.run(["busctl","--user","call","org.rsynapse.Steward","/org/rsynapse/Steward","org.rsynapse.Steward.Associations1","UnassignWorkspaceProject","t","77"],check=True)
    left=relation_rows("org.rsynapse.workspace.project")
    assert len(left)==1 and left[0][0][0]["id"]=="78"
    assert relation_rows("org.rsynapse.workspace.name")==names
    assert subprocess.check_output([str(ROOT/"steward/target/release/proj"),"project","list","--json"])==projects
    policies=relation_rows("org.rsynapse.workspace.project-policy")
    assert len(policies)==1 and policies[0][0][0]["id"]=="77" and policies[0][0][3]["automatic"]=="false" and policies[0][1] is False
    init_workspace(77,os.environ["RSYNAPSE_TEST_DIR"])
    assert relation_rows("org.rsynapse.workspace.project-policy")==[]
    assert relation_rows("org.rsynapse.workspace.name")==names
    project_id=context[1]
    proj_bin=str(ROOT/"steward/target/release/proj")
    subprocess.run([proj_bin,"remove",project_id],check=True)
    wait(lambda: relation_rows("org.rsynapse.workspace.project")==[])
    assert relation_rows("org.rsynapse.workspace.name")==names
    assert not any(p["id"]==project_id for p in json.loads(subprocess.check_output([proj_bin,"project","list","--json"],text=True)))
    subprocess.run([proj_bin,"add",os.environ["RSYNAPSE_TEST_DIR"]],check=True,stdout=subprocess.DEVNULL)
    assert relation_rows("org.rsynapse.workspace.project")==[]
    init_workspace(77,os.environ["RSYNAPSE_TEST_DIR"])
    api=subprocess.Popen([API],env=env)
    wait(lambda:call("/health"))
    assert call("/api/workday-targets")=={"span_hours":8.0,"unlocked_hours":6.0,"active_hours":4.0}
    call("/api/workday-targets","PUT",{"span_hours":8,"unlocked_hours":5.5,"active_hours":4})
    call("/api/workday-targets","PUT",{"span_hours":8,"unlocked_hours":3,"active_hours":4},400)
    goal={"id":"goal1","date":"2026-10-07","title":"Original title","kind":"outcome","project":os.environ["RSYNAPSE_TEST_DIR"],"success":"Original criterion","priority":"high","status":"planned"}
    stream=urllib.request.urlopen(base+"/api/events",timeout=5)
    assert b"connected" in stream.readline();stream.readline();stream.readline()
    call("/api/goals","POST",goal,201)
    assert b"goals-changed" in stream.readline();stream.close()
    call("/api/goals","POST",goal,409)
    rows=call("/api/goals?date=2026-10-07");assert rows==[goal]
    path="/api/goals/2026-10-07/goal1"
    call(path+"/status","PATCH",{"status":"completed"})
    edited=call(path,"PATCH",{"title":"Edited title","success":"Edited criterion"})
    assert edited["status"]=="completed" and edited["success"]=="Edited criterion"
    link={"agent":"codex","session_id":"stable-session"}
    call(path+"/links","POST",link,204);assert call(path+"/links")==[link]
    habit=call(path,"PATCH",{"kind":"habit","project":None})
    assert habit["kind"]=="habit" and call(path+"/links")==[]
    call(path+"/links","POST",link,400)
    call(path,"PATCH",{"date":"2026-10-08"},400)
    bad=dict(goal,id="../bad");call("/api/goals","POST",bad,400)
    race=dict(goal,id="race")
    def create_race(_):
        req=urllib.request.Request(base+"/api/goals",data=json.dumps(race).encode(),headers={"Content-Type":"application/json"},method="POST")
        try:
            with urllib.request.urlopen(req) as response:return response.status
        except urllib.error.HTTPError as e:return e.code
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:assert sorted(executor.map(create_race,range(2)))==[201,409]
    req=urllib.request.Request(base+"/api/goals",method="OPTIONS",headers={"Origin":"https://other.invalid","Access-Control-Request-Method":"POST","Access-Control-Request-Headers":"content-type"})
    with urllib.request.urlopen(req) as response:assert response.headers.get("Access-Control-Allow-Origin") is None
    api.terminate();api.wait(timeout=5);locus.terminate();locus.wait(timeout=5)
    steward.terminate();steward.wait(timeout=5);projd.terminate();projd.wait(timeout=5)
    projd=subprocess.Popen([PROJD],env=env)
    locus=subprocess.Popen([LOCUS],env=env)
    wait(lambda:subprocess.run(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","List","s","org.rsynapse.day.goal"],capture_output=True).returncode==0)
    steward=subprocess.Popen([STEWARD,"associations"],env=env)
    wait(lambda:subprocess.run(["busctl","--user","status","org.rsynapse.Steward"],capture_output=True).returncode==0)
    api=subprocess.Popen([API],env=env);wait(lambda:call("/health"))
    rows=call("/api/goals?date=2026-10-07");assert len(rows)==2
    assert call("/api/workday-targets")["unlocked_hours"]==5.5
    assert next(g for g in rows if g["id"]=="goal1")["success"]=="Edited criterion"
    call(path,"DELETE",expected=204);assert len(call("/api/goals?date=2026-10-07"))==1
    assert "2026-10-07" in call("/api/goal-days")["days"]
    print("PASS: workspace assignment/unassignment/name preservation, overwrite rejection, CRUD, explicit status, duplicate race, SSE, links, habit conversion, CORS and persistence")
finally:
    if api is not None and api.poll() is None:api.terminate();api.wait(timeout=5)
    if locus.poll() is None:locus.terminate();locus.wait(timeout=5)
    if steward is not None and steward.poll() is None:steward.terminate();steward.wait(timeout=5)
    if projd.poll() is None:projd.terminate();projd.wait(timeout=5)
