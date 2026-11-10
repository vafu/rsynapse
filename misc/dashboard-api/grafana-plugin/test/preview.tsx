import React, {useState} from 'react';
import {createRoot} from 'react-dom/client';
import {WorkdayTargetsPanel} from '../src/WorkdayTargetsPanel';
import {WorkdayPanel} from '../src/WorkdayPanel';
import {rangeParams} from '../src/workday-range.mjs';
import {GoalMetricsPanel} from '../src/GoalMetricsPanel';
function Preview() {
  const [version,setVersion]=useState(0);
  if(new URLSearchParams(location.search).get('mode')==='metrics')return <><GoalMetricsPanel width={1100} height={1800} options={{apiUrl:location.origin}} range={{from:1791900000000+version,to:1791986400000+version}} refreshKey={version}/><button onClick={()=>setVersion(version+3600000)}>Change dashboard range</button></>;
  if (new URLSearchParams(location.search).get('mode')!=='workday') return <WorkdayTargetsPanel width={1100} height={500} options={{apiUrl:location.origin}}/>;
  return <><WorkdayPanel width={1100} height={200} start={1791390000000} finish={1791418800000+version}
    initialSearch={location.search} setRange={(start,automatic)=>{
      (window as any).rangeChanges=((window as any).rangeChanges||0)+1;
      const url=new URL(location.href);
      for (const [key,value] of Object.entries(rangeParams(start,automatic))) {if(value===undefined) url.searchParams.delete(key);else url.searchParams.set(key,String(value));}
      history.replaceState({},'',url);
    }}/><button onClick={()=>setVersion(version+1)}>Simulate data refresh</button></>;
}
createRoot(document.getElementById('root')!).render(<Preview/>);
