import {useEffect,useRef,useState} from 'react';
import {MetricsData,Range} from './types';

export function useMetrics(api:string,range:Range,project:string,goal:string,includeSubgoals:boolean,refreshKey:number) {
  const [data,setData]=useState<MetricsData|null>(null);
  const [error,setError]=useState('');const [connected,setConnected]=useState(false);const [busy,setBusy]=useState(false);
  const [version,setVersion]=useState(0);const refresh=()=>setVersion(v=>v+1);
  const last=useRef<{api:string;from:number;to:number;project:string;goal:string;include:boolean}|null>(null);
  useEffect(()=>{
    const source=new EventSource(api+'/api/events');
    source.addEventListener('connected',()=>{setConnected(true);refresh();});
    source.addEventListener('goals-changed',refresh);source.addEventListener('goal-metrics-changed',refresh);
    source.onerror=()=>setConnected(false);
    return()=>source.close();
  },[api]);
  useEffect(()=>{
    const controller=new AbortController();
    const query={api,from:range.from,to:range.to,project,goal,include:includeSubgoals};
    // Never display a previous task/project's totals under newly selected filters.
    if (!last.current || Object.keys(query).some(k=>query[k]!==last.current?.[k])) setData(null);
    last.current=query;setBusy(true);
    const params=new URLSearchParams({from_ms:String(range.from),to_ms:String(range.live?Date.now():range.to),project_id:project,instance_id:goal,include_subgoals:String(includeSubgoals)});
    fetch(api+'/api/goal-metrics?'+params,{signal:controller.signal})
      .then(async response=>{if(!response.ok){const body=await response.json().catch(()=>({}));throw new Error(body.error||`HTTP ${response.status}`);}return response.json();})
      .then(rows=>{if(!controller.signal.aborted){setData(rows);setError('');}})
      .catch(e=>{if(!controller.signal.aborted){setData(null);setError(e.message);}})
      .finally(()=>{if(!controller.signal.aborted)setBusy(false);});
    return()=>controller.abort();
  },[api,range.from,range.to,range.live,project,goal,includeSubgoals,refreshKey,version]);
  return {data,error,connected,busy,refresh};
}
