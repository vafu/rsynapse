import React,{useId} from 'react';
import {MetricsData,Work,breadcrumb,duration,palette,timestamp} from './types';

export function WorkTimeline({data,onSelect}:{data:MetricsData;onSelect:(id:string)=>void}) {
  const pattern='gap'+useId().replace(/:/g,'');
  const lanes=new Map<string,Work[]>();
  for(const work of data.work){const key=work.project_id+'/'+work.instance_id+'/'+(work.kind==='agent'?'agent':'human');if(!lanes.has(key))lanes.set(key,[]);lanes.get(key)!.push(work);}
  const span=Math.max(1,data.to_ms-data.from_ms);
  return <section aria-label="Goal work timeline">
    <h3>When you worked on which goal</h3>
    <div style={{display:'flex',gap:18,flexWrap:'wrap',marginBottom:8}}>{Object.entries(palette).map(([kind,color])=><span key={kind}><span style={{display:'inline-block',width:12,height:12,background:color,marginRight:5}}/>{kind==='active'?'Input-active focus':kind==='idle'?'Idle focus':kind==='agent'?'Agent work':'Interrupted / unknown duration'}</span>)}</div>
    <div style={{display:'flex',justifyContent:'space-between',marginLeft:240,fontSize:12,opacity:.75}}><span>{timestamp(data.from_ms)}</span><span>{timestamp(data.to_ms)}</span></div>
    {!lanes.size && <p>No work intervals recorded in this range.</p>}
    {[...lanes].map(([key,work])=><div key={key} style={{display:'grid',gridTemplateColumns:'minmax(150px,240px) minmax(200px,1fr)',gap:12,alignItems:'center',margin:'7px 0'}}>
      <button onClick={()=>onSelect(work[0].instance_id)} title={breadcrumb(work[0].instance_id,data.identities)} style={{background:'transparent',border:0,color:'inherit',textAlign:'left',font:'inherit',cursor:'pointer',minWidth:0}}>
        <strong style={{display:'block',overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap'}}>{data.identities.find(g=>g.instance_id===work[0].instance_id)?.goal.title||work[0].goal_title}</strong>
        <small style={{opacity:.7}}>{work[0].project_name||'No project'} · {work[0].kind==='agent'?'Agent':'Human focus'}</small>
      </button>
      <svg role="img" aria-label={`Work intervals for ${work[0].goal_title}`} width="100%" height={32} viewBox="0 0 1000 32" preserveAspectRatio="none" style={{background:'#8882',borderRadius:3}}>
        <defs><pattern id={pattern+work[0].id} width={8} height={8} patternUnits="userSpaceOnUse"><path d="M-2 2 L2 -2 M0 8 L8 0 M6 10 L10 6" stroke={palette.unknown} strokeWidth={2}/></pattern></defs>
        {work.map(w=>{
          const unknown=w.end_reason.includes('unknown-duration');
          const x=Math.max(0,(w.started_at_ms-data.from_ms)/span*1000);
          const width=Math.max(1,(w.ended_at_ms-w.started_at_ms)/span*1000);
          return <rect key={w.id} data-work-kind={unknown?'unknown':w.kind} x={x} y={4} width={width} height={24} rx={2} fill={unknown?`url(#${pattern+work[0].id})`:palette[w.kind]} opacity={w.kind==='idle'?.7:1} stroke={w.open?'currentColor':'none'}>
            <title>{`${breadcrumb(w.instance_id,data.identities)}\n${w.project_name} · ${w.kind}\n${timestamp(w.started_at_ms)} → ${timestamp(w.ended_at_ms)}\n${unknown?'Duration unknown after interruption':duration(w.duration_ms)}${w.open?' · recording':''}\nRecorded as: ${w.goal_title}`}</title>
          </rect>;
        })}
      </svg>
    </div>)}
    <small style={{opacity:.75}}>Intervals are recorded activity, not time inferred from an “in progress” status. Locked and unobserved time are not added to focus effort.</small>
  </section>;
}
