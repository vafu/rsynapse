import React,{useRef,useState} from 'react';
import {Range,Stats,control,duration,timestamp,breadcrumb} from './metrics/types';
import {useMetrics} from './metrics/useMetrics';
import {WorkTimeline} from './metrics/WorkTimeline';
import {ProjectEffort} from './metrics/ProjectEffort';

type Props={width:number;height:number;range:Range;refreshKey?:number;options?:{apiUrl?:string}};
const cell:React.CSSProperties={textAlign:'left',padding:'8px 10px',borderBottom:'1px solid #8884',verticalAlign:'top'};
const number:React.CSSProperties={...cell,whiteSpace:'nowrap',fontVariantNumeric:'tabular-nums'};
export function GoalMetricsPanel({width,height,range,refreshKey=0,options}:Props) {
  const api=(options?.apiUrl||'http://127.0.0.1:8770').replace(/\/$/,'');
  const [project,setProject]=useState('');const [goal,setGoal]=useState('');const [include,setInclude]=useState(true);
  const [allTasks,setAllTasks]=useState(false);const [eventLimit,setEventLimit]=useState(50);
  const {data,error,connected,busy,refresh}=useMetrics(api,range,project,goal,include,refreshKey);
  // Choices remain available while loading a newly selected scope.
  const catalog=useRef<typeof data>(null);
  if(data)catalog.current=data;
  const choices=data||catalog.current;
  const totals={active:0,idle:0,agent:0,interrupted:0};
  for(const w of data?.work||[]){totals[w.kind]+=w.duration_ms;if(w.end_reason.includes('unknown-duration'))totals.interrupted++;}
  const history=[...(data?.history||[])].sort((a,b)=>b.at_ms-a.at_ms||b.sequence-a.sequence);
  const finished=new Set(history.filter(e=>(e.kind==='status-changed'||e.kind==='created')&&e.goal.status==='completed'&&e.old_status!=='completed').map(e=>e.instance_id));
  const activity=new Set([...(data?.work||[]).flatMap(w=>include?[w.instance_id,...w.ancestor_ids]:[w.instance_id]),...history.filter(e=>e.kind!=='observed').map(e=>e.instance_id)]);
  const rows=(data?.stats||[]).filter(s=>allTasks||activity.has(s.instance_id));
  const choose=(id:string)=>{setGoal(id);setEventLimit(50);};
  const lifetime=(s:Stats,kind:'lead'|'cycle')=>s.completed_at_ms&&(kind==='lead'?s.created_at_ms:s.first_started_at_ms)?duration(kind==='lead'?s.lead_time_ms:s.cycle_time_ms):'Unknown';
  return <div style={{width,height,overflow:'auto',boxSizing:'border-box',padding:14,color:'inherit'}}>
    <div style={{display:'flex',alignItems:'center',gap:12,flexWrap:'wrap',marginBottom:12}}>
      <label>Project <select aria-label="Metrics project" value={project} onChange={e=>{setProject(e.target.value);setGoal('');}} style={{...control,maxWidth:280}}><option value="">All projects</option>{choices?.projects.map(p=><option key={p.id} value={p.id}>{p.name||p.id}</option>)}</select></label>
      <label>Goal / subgoal <select aria-label="Metrics goal" value={goal} onChange={e=>choose(e.target.value)} style={{...control,maxWidth:480}}><option value="">All tasks</option>{choices?.identities.filter(g=>!project||g.goal.project_id===project||choices.work.some(w=>w.instance_id===g.instance_id&&w.project_id===project)||choices.stats.some(s=>s.instance_id===g.instance_id&&s.project_id===project)).map(g=><option key={g.instance_id} value={g.instance_id}>{breadcrumb(g.instance_id,choices.identities)}{g.deleted_at_ms?' [removed]':''} · {g.instance_id.slice(0,6)}</option>)}</select></label>
      <label><input type="checkbox" checked={include} onChange={e=>setInclude(e.target.checked)}/> Include subgoals</label>
      <button style={{...control,cursor:'pointer'}} disabled={busy} onClick={refresh}>Refresh metrics</button>
      <small role="status">{busy?'Loading…':connected?'Live D-Bus updates':'Connecting to local API…'}</small>
    </div>
    {error&&<div role="alert" style={{color:'#f2495c',whiteSpace:'pre-wrap'}}>{error}</div>}
    {data&&<>
      <section aria-label="Goal metric totals" style={{display:'flex',gap:28,flexWrap:'wrap',margin:'16px 0 24px'}}>{[
        ['Input-active focus',duration(totals.active)],['Idle focus',duration(totals.idle)],['Unlocked focus',duration(totals.active+totals.idle)],['Agent work',duration(totals.agent)],['Tasks completed in range',String(finished.size)],['Interrupted spans',String(totals.interrupted)],
      ].map(([label,value])=><div key={label}><div style={{opacity:.75}}>{label}</div><strong style={{fontSize:26}}>{value}</strong></div>)}</section>
      <WorkTimeline data={data} onSelect={choose}/>
      <ProjectEffort work={data.work}/>
      <section aria-label="Goal effort and completion">
        <h3>Effort and completion timing</h3>
        <label><input type="checkbox" checked={allTasks} onChange={e=>setAllTasks(e.target.checked)}/> Show tasks without activity in this range</label>
        <p><small>Effort/status durations follow the selected range; creation, start and last completion are lifetime facts. {include?'Parent rows include subgoal effort; summary totals count each interval once.':'Rows show direct effort only.'} Unknown timestamps predate recording. Agent time is separate; automatic collection currently records human focus.</small></p>
        <div style={{overflowX:'auto'}}><table style={{width:'100%',borderCollapse:'collapse'}}><thead><tr>{['Project / goal','Status','Input-active','Idle','Agent','In progress (recorded)','Created','First started','Last completed','Lead time','Start → completion','History'].map(h=><th key={h} style={cell}>{h}</th>)}</tr></thead><tbody>{rows.map(s=><tr key={s.instance_id}>
          <td style={{...cell,minWidth:230,maxWidth:420}}><button style={{padding:0,border:0,background:'transparent',color:'inherit',font:'inherit',textAlign:'left',cursor:'pointer'}} title={breadcrumb(s.instance_id,data.identities)} onClick={()=>choose(s.instance_id)}><strong>{s.title}</strong></button><small style={{display:'block',opacity:.7}}>{s.project_name||'No project'} · {s.key}</small>{data.identities.find(g=>g.instance_id===s.instance_id)?.deleted_at_ms?<small>Removed task instance</small>:null}</td>
          <td style={cell}>{s.status}</td><td style={number}>{duration(s.active_ms)}</td><td style={number}>{duration(s.idle_ms)}</td><td style={number}>{duration(s.agent_ms)}</td><td style={number}>{duration(s.in_progress_ms)}</td>
          <td style={cell}>{timestamp(s.created_at_ms)}</td><td style={cell}>{timestamp(s.first_started_at_ms)}</td><td style={cell}>{timestamp(s.completed_at_ms)}</td><td style={number}>{lifetime(s,'lead')}</td><td style={number}>{lifetime(s,'cycle')}</td>
          <td style={cell}>{s.history_partial?'Partial':'Recorded'}{s.created_at_ms===0?' · pre-recording goal':''}{s.interrupted_sessions?` · ${s.interrupted_sessions} interrupted`:''}<small style={{display:'block',opacity:.7}}>Observed {timestamp(s.observed_at_ms)}</small></td>
        </tr>)}</tbody></table></div>
        {!rows.length&&<p>No goals with recorded activity or lifecycle changes in this range.</p>}
      </section>
      <section aria-label="Goal lifecycle history"><h3>Lifecycle and selection history</h3><div style={{overflowX:'auto'}}><table style={{width:'100%',borderCollapse:'collapse'}}><thead><tr>{['When','Event','Goal','State transition'].map(h=><th key={h} style={cell}>{h}</th>)}</tr></thead><tbody>{history.slice(0,eventLimit).map(e=><tr key={e.sequence}><td style={number}>{timestamp(e.at_ms)}</td><td style={cell}>{e.kind}</td><td style={cell}><button style={{border:0,background:'transparent',color:'inherit',font:'inherit',cursor:'pointer'}} onClick={()=>choose(e.instance_id)}>{e.goal.title}</button></td><td style={cell}>{e.old_status?`${e.old_status} → `:''}{e.goal.status}</td></tr>)}</tbody></table></div>{!history.length&&<p>No lifecycle changes recorded in this range.</p>}{history.length>eventLimit&&<button style={control} onClick={()=>setEventLimit(n=>n+50)}>Show more events</button>}</section>
      <p><small>Authoritative history: projd SQLite via dashboard-api. Last query: {timestamp(data.queried_at_ms)}. Interval/query refresh does not create recording heartbeats.</small></p>
    </>}
  </div>;
}
