"""Explicit, idempotent legacy locus project/goal import; keeps a durable backup."""
import datetime, json, os, pathlib, subprocess, tempfile

PROJ=os.environ.get("PROJ_BIN",str(pathlib.Path.home()/".local/bin/proj"))
def records(relation):
    return json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","List","s",relation],text=True))["data"][0]
def states(relation):
    return json.loads(subprocess.check_output(["busctl","--user","--json=short","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","ListWithPersistence","s",relation],text=True))["data"][0]
def args_map(value):
    out=[str(len(value))]
    for key,val in value.items():out.extend([key,str(val)])
    return out
def bind_record(row,relation,metadata,persist):
    args=args_map(row[0])+[relation]+args_map(row[2])+args_map(metadata)+["true" if persist else "false"]
    method="SetWithPersistence"
    subprocess.run(["busctl","--user","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1",method,"a{ss}sa{ss}a{ss}b",*args],check=True,stdout=subprocess.DEVNULL)
rows=records("org.rsynapse.day.goal")
workspace_states=states("org.rsynapse.workspace.project")
projects=[r for r,persist in workspace_states]+records("org.rsynapse.project.metadata")
legacy=[]
for row in rows:
    if "goal" in row[3]:legacy.append(json.loads(row[3]["goal"]))
state=pathlib.Path(os.environ.get("XDG_STATE_HOME",str(pathlib.Path.home()/".local/state")))/"rsynapse/projd"
state.mkdir(parents=True,exist_ok=True)
backup=state/("locus-import-"+datetime.datetime.now().strftime("%Y%m%d-%H%M%S-%f")+".json")
with backup.open("x") as f:
    json.dump({"goals":legacy,"relations":rows,"projects":projects,"workspace_states":workspace_states},f,indent=2)
    f.flush();os.fsync(f.fileno())
fd=os.open(state,os.O_RDONLY|os.O_DIRECTORY)
try:os.fsync(fd)
finally:os.close(fd)
paths=set()
for row,persist in workspace_states:
    target=row[2]
    if target.get("kind")=="org.rsynapse.project.path" and pathlib.Path(target["id"]).is_dir():paths.add(target["id"])
for path in sorted(paths):subprocess.run([PROJ,"add",path],check=True)
with tempfile.NamedTemporaryFile(mode="w",suffix=".json") as f:
    json.dump(legacy,f);f.flush();subprocess.run([PROJ,"goal","import",f.name],check=True)
goals=json.loads(subprocess.check_output([PROJ,"goal","list","--all","--json"],text=True))
bykey={(g["date"],g["id"]):g for g in goals}
for goal in legacy:
    expected={"kind":"outcome","status":"planned"}
    expected.update(goal)
    if bykey.get((goal["date"],goal["id"]))!=expected:
        raise RuntimeError(f"Imported goal content differs: {goal['date']}/{goal['id']}; legacy relations retained")
for row in rows:
    if "goal" in row[3]:bind_record(row,"org.rsynapse.day.goal",{"managed-by":"rsynapse-steward"},True)
for row,persist in workspace_states:
    root=row[2].get("id")
    if root not in paths:continue
    cwd=row[3].get("cwd-path",root)
    if not pathlib.Path(cwd).is_dir():cwd=root
    snapshot=json.loads(subprocess.check_output([PROJ,"metadata",cwd,"--json"],text=True))
    bind_record(row,"org.rsynapse.workspace.project",{"managed-by":"rsynapse-steward","project-id":snapshot["id"],"checkout-id":snapshot["checkout-id"]},persist)
# Legacy self-mirrored project metadata is now backed up and owned by projd.
for row in projects:
    if row[1]=="org.rsynapse.project.metadata" and row[2].get("id") in paths:
        args=args_map(row[0])+[row[1]]+args_map(row[2])
        subprocess.run(["busctl","--user","call","org.rsynapse.Locus","/org/rsynapse/Locus","org.rsynapse.Locus.Relations1","Unset","a{ss}sa{ss}",*args],check=True,stdout=subprocess.DEVNULL)
print(f"Imported {len(paths)} project/checkouts and verified {len(legacy)} saved goals. Backup: {backup}")
