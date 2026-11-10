import React, {useEffect, useRef, useState} from 'react';

type Targets = {span_hours:number; unlocked_hours:number; active_hours:number};
type Props = {width?:number; height?:number; options?:{apiUrl?:string}};
const control:React.CSSProperties = {color:'inherit',background:'transparent',border:'1px solid #8888',borderRadius:4,padding:'5px 8px',font:'inherit'};
const button:React.CSSProperties = {...control,cursor:'pointer'};

export function WorkdayTargetsPanel({width,height,options}:Props) {
  const api=(options?.apiUrl||'http://127.0.0.1:8770').replace(/\/$/,'');
  const [targets,setTargets]=useState<Targets|null>(null);
  const [draft,setDraft]=useState<Targets|null>(null);
  const [error,setError]=useState('');
  const [busy,setBusy]=useState(false);
  const [connected,setConnected]=useState(false);
  const sequence=useRef(0);
  async function request(method='GET',body?:Targets):Promise<Targets> {
    const response=await fetch(api+'/api/workday-targets',{method,headers:body?{'Content-Type':'application/json'}:{},body:body?JSON.stringify(body):undefined});
    if(!response.ok) {
      const text=await response.text();let message=text;
      try {message=JSON.parse(text).error||text;}catch {}
      throw new Error(message||`HTTP ${response.status}`);
    }
    return response.json();
  }
  async function load() {
    const current=++sequence.current;
    try {const value=await request();if(current===sequence.current){setTargets(value);setError('');}}
    catch(e){if(current===sequence.current)setError((e as Error).message);}
  }
  useEffect(()=>{
    const source=new EventSource(api+'/api/events');
    source.addEventListener('connected',()=>{setConnected(true);void load();});
    source.addEventListener('workday-targets-changed',()=>{void load();});
    source.onerror=()=>setConnected(false);
    void load();
    return ()=>{source.close();sequence.current++;};
  },[api]);
  async function save(event:React.FormEvent) {
    event.preventDefault();if(!draft)return;
    setBusy(true);setError('');
    try {await request('PUT',draft);setDraft(null);await load();}
    catch(e){setError((e as Error).message);}finally{setBusy(false);}
  }
  return <section style={{width,height,overflow:'auto',padding:12,boxSizing:'border-box',color:'inherit'}}>
    <p>Daily workday targets {targets&&<strong>{targets.span_hours}h span · {targets.unlocked_hours}h unlocked · {targets.active_hours}h active</strong>}</p>
    <div style={{display:'flex',gap:8,alignItems:'center'}}>
      <button style={button} disabled={busy||!targets} onClick={()=>setDraft({...targets!})}>Edit workday targets</button>
      <button style={button} disabled={busy} onClick={()=>void load()}>Refresh</button>
      <small>{connected?'Live D-Bus updates':'Connecting to local API…'}</small>
    </div>
    {error&&<p role="alert" style={{color:'#f2495c',whiteSpace:'pre-wrap'}}>{error}</p>}
    {draft&&<form onSubmit={save} style={{display:'flex',gap:12,alignItems:'end',flexWrap:'wrap',marginTop:12}}>
      {([['span_hours','Elapsed span hours'],['unlocked_hours','Unlocked hours'],['active_hours','Active hours']] as const).map(([key,label])=><label key={key} style={{display:'flex',flexDirection:'column'}}>{label}<input aria-label={label} type="number" min={0} max={24} step={0.25} required disabled={busy} style={control} value={draft[key]} onChange={e=>setDraft({...draft,[key]:Number(e.target.value)})}/></label>)}
      <button style={button} type="submit" disabled={busy}>Save workday targets</button>
      <button style={button} type="button" disabled={busy} onClick={()=>setDraft(null)}>Cancel targets</button>
    </form>}
    <p><small>Changes apply today and future days. Earlier log rows keep their recorded targets. Active hours cannot exceed unlocked hours.</small></p>
  </section>;
}
