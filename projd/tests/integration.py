"""Projd/CLI work without any shell, locus, agent or Grafana service."""
import json,os,pathlib,subprocess,sys,tempfile,time
ROOT=pathlib.Path(__file__).resolve().parents[2]
PROJD=os.environ.get("PROJD_BIN",str(ROOT/"steward/target/release/projd"))
PROJ=os.environ.get("PROJ_BIN",str(ROOT/"steward/target/release/proj"))
if "--child" not in sys.argv:
    with tempfile.TemporaryDirectory(prefix="projd-test-") as temp:
        env=dict(os.environ,PROJD_STORE_PATH=temp+"/store.sqlite3",XDG_DATA_HOME=temp,XDG_DATA_DIRS=temp,TEST_PROJECT_DIR=temp)
        subprocess.run(["dbus-run-session","--",sys.executable,__file__,"--child"],env=env,check=True)
    sys.exit(0)
def cli(*args):return subprocess.check_output([PROJ,*args],text=True)
def wait(fn):
    until=time.monotonic()+10
    while time.monotonic()<until:
        try:
            result=fn()
            if result:return result
        except Exception:pass
        time.sleep(.02)
    raise AssertionError("condition not observed")
temp=pathlib.Path(os.environ["TEST_PROJECT_DIR"]);repo=temp/"repo";repo.mkdir()
def git(*args):subprocess.run(["git","-C",str(repo),*args],check=True,stdout=subprocess.DEVNULL)
git("init","-b","main");(repo/"file.txt").write_text("one\n");git("add","file.txt");git("-c","user.name=Fixture","-c","user.email=fixture@example.invalid","commit","-m","Initial")
daemon=subprocess.Popen([PROJD])
try:
    wait(lambda:cli("project","list","--json"))
    project=cli("add",str(repo),"--name","Fixture project").strip();assert project
    assert cli("project","add",str(repo)).strip()==project
    assert json.loads(cli("project","list","--json"))[0]["name"]=="Fixture project"
    first=json.loads(cli("metadata",str(repo),"--json"));assert first["id"]==project and first["branch"]=="main"
    alternate=temp/"alternate";git("worktree","add","-b","alternate",str(alternate))
    assert cli("add",str(alternate)).strip()==project
    other=json.loads(cli("metadata",str(alternate),"--json"));assert other["id"]==project
    assert other["checkout-id"]!=first["checkout-id"] and other["context-id"]!=first["context-id"]
    child=repo/"nested";child.mkdir();nested=json.loads(cli("metadata",str(child),"--json"));assert nested["context-id"]!=first["context-id"] and nested["relative-cwd"]=="nested"
    cli("goal","add","habit1","--kind","habit","--title","Personal budget","--success","At most 30 minutes","--date","2026-10-07")
    cli("goal","status","habit1","completed","--date","2026-10-07")
    goals=json.loads(cli("goal","list","--all","--json"));assert goals[0]["status"]=="completed"
    git("checkout","-b","new-branch")
    # Read only lists; do not invoke Refresh, proving Git control-file watching.
    wait(lambda:any(c["branch"]=="new-branch" for c in json.loads(cli("checkout","list","--json"))))
    objects=json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Proj","/org/rsynapse/Proj","org.freedesktop.DBus.ObjectManager","GetManagedObjects"],text=True))["data"][0]
    assert any("org.rsynapse.Proj.Project1" in i for i in objects.values())
    assert any("org.rsynapse.Proj.Context1" in i for i in objects.values())
    assert any("org.rsynapse.Proj.Goal1" in i for i in objects.values())
    daemon.terminate();daemon.wait(timeout=5);daemon=subprocess.Popen([PROJD])
    wait(lambda:json.loads(cli("project","list","--json")))
    assert json.loads(cli("project","list","--json"))[0]["id"]==project
    assert json.loads(cli("goal","list","--all","--json"))==goals
    saved_checkouts=json.loads(cli("checkout","list","--json"))
    original_file=(repo/"file.txt").read_bytes();original_stat=(repo/"file.txt").stat().st_mtime_ns
    cli("remove",project)
    assert json.loads(cli("project","list","--json"))==[] and json.loads(cli("checkout","list","--json"))==[]
    for command in ["metadata","update","root"]:
        blocked=subprocess.run([PROJ,command,str(alternate)],capture_output=True,text=True)
        assert blocked.returncode!=0 and "not registered" in blocked.stderr
    objects=json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Proj","/org/rsynapse/Proj","org.freedesktop.DBus.ObjectManager","GetManagedObjects"],text=True))["data"][0]
    assert not any("org.rsynapse.Proj.Project1" in i or "org.rsynapse.Proj.Context1" in i for i in objects.values())
    assert json.loads(cli("goal","list","--all","--json"))==goals
    assert (repo/"file.txt").read_bytes()==original_file and (repo/"file.txt").stat().st_mtime_ns==original_stat
    assert (alternate/"file.txt").exists()
    daemon.terminate();daemon.wait(timeout=5);daemon=subprocess.Popen([PROJD])
    wait(lambda:cli("project","list","--json"))
    assert json.loads(cli("project","list","--json"))==[]
    assert json.loads(cli("checkout","list","--json"))==[]
    new_project=cli("add",str(repo)).strip();assert new_project!=project
    assert cli("add",str(alternate)).strip()==new_project
    assert json.loads(cli("metadata",str(child),"--json"))["context-id"]!=nested["context-id"]
    assert json.loads(cli("goal","list","--all","--json"))==goals
    print("PASS: project/worktree/context identity, ObjectManager, reactive Git, goals, permanent metadata-only removal, no read/refresh resurrection and explicit re-registration")
finally:daemon.terminate();daemon.wait(timeout=5)
