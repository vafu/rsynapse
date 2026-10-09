import React, {useState} from 'react';
import {createRoot} from 'react-dom/client';
import {GoalsPanel} from '../src/GoalsPanel';
import {WorkdayPanel} from '../src/WorkdayPanel';
import {rangeParams} from '../src/workday-range.mjs';
function Preview() {
  const [version,setVersion]=useState(0);
  if (new URLSearchParams(location.search).get('mode')!=='workday') return <GoalsPanel width={1100} height={1000} options={{apiUrl:location.origin}}/>;
  return <><WorkdayPanel width={1100} height={200} start={1791390000000} finish={1791418800000+version}
    initialSearch={location.search} setRange={(start,automatic)=>{
      (window as any).rangeChanges=((window as any).rangeChanges||0)+1;
      const url=new URL(location.href);
      for (const [key,value] of Object.entries(rangeParams(start,automatic))) {if(value===undefined) url.searchParams.delete(key);else url.searchParams.set(key,String(value));}
      history.replaceState({},'',url);
    }}/><button onClick={()=>setVersion(version+1)}>Simulate data refresh</button></>;
}
createRoot(document.getElementById('root')!).render(<Preview/>);
