import React, { useEffect, useRef, useState } from 'react';

type Goal = { id: string; date: string; title: string; kind: 'outcome'|'habit'; project: string|null; success: string; priority: string; status: string };
type Link = { agent: string; session_id: string };
type Session = Link & {title: string; state: string; cwd: string};
type Targets = {span_hours:number;unlocked_hours:number;active_hours:number};
type Props = { width?: number; height?: number; options?: {apiUrl?: string} };
const statuses = ['planned', 'in-progress', 'completed', 'deferred'];
const control: React.CSSProperties = { color: 'inherit', background: 'transparent', border: '1px solid #8888', borderRadius: 4, padding: '5px 8px', font: 'inherit' };
const button: React.CSSProperties = {...control, cursor: 'pointer'};
const localDate = () => {const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;};

export function GoalsPanel({height, width, options}: Props) {
  const api = (options?.apiUrl || 'http://127.0.0.1:8770').replace(/\/$/,'');
  const [date, setDate] = useState(localDate);
  const [days, setDays] = useState<string[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [projects, setProjects] = useState<string[]>([]);
  const [draft, setDraft] = useState<Goal|null>(null);
  const [original, setOriginal] = useState<Goal|null>(null);
  const [linkGoal, setLinkGoal] = useState<Goal|null>(null);
  const [links, setLinks] = useState<Link[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [selectedSession, setSelectedSession] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [connected, setConnected] = useState(false);
  const [targets,setTargets]=useState<Targets>({span_hours:8,unlocked_hours:6,active_hours:4});
  const [targetDraft,setTargetDraft]=useState<Targets|null>(null);
  const sequence = useRef(0);

  async function request(path: string, method = 'GET', body?: unknown) {
    const response = await fetch(api+path, {method, headers: body === undefined ? {} : {'Content-Type':'application/json'}, body:body===undefined?undefined:JSON.stringify(body)});
    if (!response.ok) { const text = await response.text(); let message=text; try {message=JSON.parse(text).error||text;} catch {} throw new Error(message || `HTTP ${response.status}`); }
    return response.status===204?null:response.json();
  }
  async function load() {
    const current = ++sequence.current;
    try {
      const [rows, calendar, roots, hours] = await Promise.all([request(`/api/goals?date=${encodeURIComponent(date)}`),request('/api/goal-days'),request('/api/projects'),request('/api/workday-targets')]);
      if (current!==sequence.current) return;
      setGoals(rows); setDays(calendar.days); setProjects(roots); setTargets(hours); setError('');
    } catch (e) {if (current===sequence.current) setError((e as Error).message);}
  }
  useEffect(() => {
    // Subscribe first, then fetch. No polling: locus changes trigger an SSE refresh.
    const source = new EventSource(api+'/api/events');
    source.addEventListener('connected', () => {setConnected(true); void load();});
    source.addEventListener('goals-changed', () => {void load();});
    source.onerror = () => {setConnected(false);};
    void load();
    return () => {source.close(); sequence.current++;};
  }, [api, date]);

  const path = (goal: Goal) => `/api/goals/${encodeURIComponent(goal.date)}/${encodeURIComponent(goal.id)}`;
  async function mutate(action: () => Promise<unknown>) {
    setBusy(true); setError('');
    try {await action(); await load();} catch (e) {setError((e as Error).message);} finally {setBusy(false);}
  }
  function startNew() {
    setOriginal(null); setLinkGoal(null);
    setDraft({id:`g-${Date.now().toString(36)}-${Math.random().toString(36).slice(2,6)}`,date,title:'',kind:'outcome',project:null,success:'',priority:'medium',status:'planned'});
  }
  async function save(event: React.FormEvent) {
    event.preventDefault(); if (!draft) return;
    await mutate(async () => {
      if (original) {
        const patch: Record<string,unknown> = {};
        for (const key of ['title','kind','project','success','priority','status'] as const) if (draft[key]!==original[key]) patch[key]=draft[key];
        await request(path(original),'PATCH',patch);
      } else {await request('/api/goals','POST',draft);}
      setDraft(null); setOriginal(null);
    });
  }
  async function showLinks(goal: Goal) {
    setDraft(null); setLinkGoal(goal); setError('');
    try {const [rows, current] = await Promise.all([request(path(goal)+'/links'),request('/api/sessions')]); setLinks(rows);setSessions(current);setSelectedSession('');} catch(e) {setError((e as Error).message);}
  }
  function field(label: string, key: keyof Goal, element: React.ReactNode) {
    return <label style={{display:'flex',flexDirection:'column',gap:4}}>{label}{element}</label>;
  }
  return <div style={{width, height, overflow:'auto', padding:12, boxSizing:'border-box', color:'inherit'}}>
    <div style={{display:'flex',gap:10,alignItems:'center',flexWrap:'wrap',marginBottom:12}}>
      <label>Goal day <input aria-label="Goal day" type="date" list="rsynapse-goal-days" value={date} disabled={busy} onChange={e=>{setDate(e.target.value);setDraft(null);setLinkGoal(null);}} style={control}/></label>
      <datalist id="rsynapse-goal-days">{days.map(day=><option key={day} value={day}/>)}</datalist>
      <button style={button} disabled={busy} onClick={()=>setDate(localDate())}>Today</button>
      <button style={button} onClick={startNew} disabled={busy}>New goal</button>
      <button style={button} onClick={()=>void load()} disabled={busy}>Refresh</button>
      <span>{goals.filter(g=>g.status==='completed').length} / {goals.length} completed</span>
      <small>{connected?'Live locus updates':'Connecting to local API…'}</small>
    </div>
    {error && <div role="alert" style={{color:'#f2495c',whiteSpace:'pre-wrap',marginBottom:12}}>{error}</div>}
    <div style={{marginBottom:12}}>Workday targets: <strong>{targets.span_hours}h span · {targets.unlocked_hours}h unlocked · {targets.active_hours}h active</strong>{' '}
      <button style={button} disabled={busy} onClick={()=>setTargetDraft({...targets})}>Edit workday targets</button>
    </div>
    {targetDraft && <form onSubmit={event=>{event.preventDefault();void mutate(async()=>{await request('/api/workday-targets','PUT',targetDraft);setTargetDraft(null);});}} style={{display:'flex',gap:12,alignItems:'end',flexWrap:'wrap',padding:12,border:'1px solid #8888',marginBottom:12}}>
      {([['span_hours','Elapsed span hours'],['unlocked_hours','Unlocked hours'],['active_hours','Active hours']] as const).map(([key,label])=><label key={key} style={{display:'flex',flexDirection:'column'}}>{label}<input aria-label={label} type="number" min={0} max={24} step={0.25} required style={control} value={targetDraft[key]} onChange={e=>setTargetDraft({...targetDraft,[key]:Number(e.target.value)})}/></label>)}
      <button style={button} type="submit" disabled={busy}>Save workday targets</button><button style={button} type="button" onClick={()=>setTargetDraft(null)}>Cancel targets</button>
      <small>Changes apply today and future days. Earlier log rows keep their recorded targets.</small>
    </form>}
    {draft && <form onSubmit={save} style={{border:'1px solid #8888',padding:12,marginBottom:12,display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(200px,1fr))',gap:10}}>
      {field('ID','id',<input aria-label="Goal ID" style={control} value={draft.id} disabled={!!original} pattern="[A-Za-z0-9_-]{1,64}" required onChange={e=>setDraft({...draft,id:e.target.value})}/>)}
      {field('Title','title',<input aria-label="Title" style={control} value={draft.title} required onChange={e=>setDraft({...draft,title:e.target.value})}/>)}
      {field('Kind','kind',<select aria-label="Kind" style={control} value={draft.kind} onChange={e=>setDraft({...draft,kind:e.target.value as Goal['kind'],project:e.target.value==='habit'?null:draft.project})}><option value="outcome">Outcome</option><option value="habit">Habit</option></select>)}
      {field('Priority','priority',<select aria-label="Priority" style={control} value={draft.priority} onChange={e=>setDraft({...draft,priority:e.target.value})}>{['low','medium','high'].map(p=><option key={p}>{p}</option>)}</select>)}
      {field('Status','status',<select aria-label="Status" style={control} value={draft.status} onChange={e=>setDraft({...draft,status:e.target.value})}>{statuses.map(s=><option key={s}>{s}</option>)}</select>)}
      {draft.kind==='outcome' && field('Project (absolute path)','project',<><input aria-label="Project" list="rsynapse-projects" style={control} value={draft.project||''} required onChange={e=>setDraft({...draft,project:e.target.value||null})}/><datalist id="rsynapse-projects">{projects.map(p=><option key={p} value={p}/>)}</datalist></>)}
      <label style={{gridColumn:'1 / -1'}}>Success criterion<textarea aria-label="Success criterion" style={{...control,display:'block',width:'100%',boxSizing:'border-box',minHeight:80}} value={draft.success} required onChange={e=>setDraft({...draft,success:e.target.value})}/></label>
      <div style={{display:'flex',gap:8}}><button type="submit" style={button} disabled={busy}>{original?'Save changes':'Create goal'}</button><button type="button" style={button} onClick={()=>setDraft(null)}>Cancel</button></div>
    </form>}
    {!goals.length && !draft && <p>No goals for this day. Add an outcome or a project-independent habit.</p>}
    {!!goals.length && <table style={{width:'100%',borderCollapse:'collapse'}}><thead><tr>{['Goal','Kind / priority','Status','Actions'].map(h=><th key={h} style={{textAlign:'left',padding:8,borderBottom:'1px solid #8888'}}>{h}</th>)}</tr></thead><tbody>{goals.map(goal=><tr key={goal.id}>
      <td style={{padding:8,borderBottom:'1px solid #8885',maxWidth:600}}><strong>{goal.title}</strong><small style={{display:'block'}}>{goal.id}{goal.project?` · ${goal.project}`:''}</small><details><summary>Success criterion</summary><p style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere'}}>{goal.success}</p></details></td>
      <td style={{padding:8}}>{goal.kind}<br/>{goal.priority}</td>
      <td style={{padding:8}}><select aria-label={`Status for ${goal.id}`} style={control} value={goal.status} disabled={busy} onChange={e=>void mutate(()=>request(path(goal)+'/status','PATCH',{status:e.target.value}))}>{statuses.map(s=><option key={s}>{s}</option>)}</select></td>
      <td style={{padding:8}}><div style={{display:'flex',gap:6,flexWrap:'wrap'}}><button style={button} disabled={busy} onClick={()=>{setDraft({...goal});setOriginal(goal);setLinkGoal(null);}}>Edit</button>{goal.kind==='outcome' && <button style={button} disabled={busy} onClick={()=>void showLinks(goal)}>Sessions</button>}<button aria-label={`Delete ${goal.id}`} style={button} disabled={busy} onClick={()=>void mutate(()=>request(path(goal),'DELETE'))}>Delete</button></div></td>
    </tr>)}</tbody></table>}
    {linkGoal && <section style={{border:'1px solid #8888',padding:12,marginTop:12}}><h4>Agent sessions for {linkGoal.title}</h4>
      <p>Matching project paths are associated automatically. Explicit links are useful for sessions working elsewhere.</p>
      <ul>{links.map(link=><li key={link.agent+'/'+link.session_id}>{link.agent} / {link.session_id} <button style={button} disabled={busy} onClick={()=>void mutate(async()=>{await request(path(linkGoal)+'/links','DELETE',link);setLinks(await request(path(linkGoal)+'/links'));})}>Unlink</button></li>)}</ul>
      <select aria-label="Agent session" value={selectedSession} style={control} onChange={e=>setSelectedSession(e.target.value)}><option value="">Select a live root session</option>{sessions.map((s,i)=><option key={s.agent+'/'+s.session_id} value={i}>{s.agent}: {s.title||s.session_id} ({s.state})</option>)}</select>
      <button style={button} disabled={busy||selectedSession===''} onClick={()=>void mutate(async()=>{const s=sessions[Number(selectedSession)];await request(path(linkGoal)+'/links','POST',{agent:s.agent,session_id:s.session_id});setLinks(await request(path(linkGoal)+'/links'));})}>Link session</button>
      <button style={button} onClick={()=>setLinkGoal(null)}>Close</button>
    </section>}
    <p><small>Goals are stored in locus. Completion is explicit. Habits never grant agent-work credit. Activity collection remains in steward; this panel does not finalize a productivity score.</small></p>
  </div>;
}
