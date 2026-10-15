import React from 'react';
import {Work,duration,palette} from './types';

export function ProjectEffort({work}:{work:Work[]}) {
  const projects=new Map<string,{name:string;active:number;idle:number;agent:number}>();
  for(const w of work){const p=projects.get(w.project_id)||{name:w.project_name||'No project',active:0,idle:0,agent:0};p[w.kind]+=w.duration_ms;projects.set(w.project_id,p);}
  const rows=[...projects].sort((a,b)=>(b[1].active+b[1].idle)-(a[1].active+a[1].idle));
  const max=Math.max(1,...rows.map(([,p])=>p.active+p.idle));
  return <section aria-label="Effort by project"><h3>Recorded focus by project</h3>{rows.map(([id,p])=><div key={id} style={{display:'grid',gridTemplateColumns:'minmax(100px,180px) minmax(150px,1fr) minmax(100px,260px)',alignItems:'center',gap:10,margin:'8px 0'}}>
    <span>{p.name}</span><div role="img" aria-label={`${p.name}: ${duration(p.active)} active and ${duration(p.idle)} idle`} style={{height:16,display:'flex',background:'#8882'}}>
      <div style={{width:`${p.active/max*100}%`,background:palette.active}}/><div style={{width:`${p.idle/max*100}%`,background:palette.idle,opacity:.7}}/>
    </div><small>{duration(p.active)} active · {duration(p.idle)} idle{p.agent?` · ${duration(p.agent)} agent (separate)`:''}</small>
  </div>)}</section>;
}
