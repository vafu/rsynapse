import React, {useEffect, useRef} from 'react';
import {shouldDefaultRange} from './workday-range.mjs';

type Props = {
  width: number; height: number; start?: number; finish?: number;
  initialSearch: string; autoStartRange?: boolean;
  setRange: (start: number, automatic: boolean) => void;
};
const clock = (value?: number) => value && Number.isFinite(value) ? new Date(value).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'}) : 'Not started';
export function WorkdayPanel({width,height,start,finish,initialSearch,autoStartRange=true,setRange}:Props) {
  const intent=useRef(shouldDefaultRange(initialSearch));
  const handled=useRef(false);
  useEffect(()=>{
    if (handled.current || !start || !Number.isFinite(start)) return;
    handled.current=true;
    if (autoStartRange && intent.current) setRange(start,true);
  },[start,autoStartRange,setRange]);
  return <div style={{width,height,display:'flex',gap:30,alignItems:'center',justifyContent:'space-around',padding:12,boxSizing:'border-box'}}>
    <button title="Use started today → now" disabled={!start} onClick={()=>start&&setRange(start,false)} style={{border:0,background:'transparent',color:'inherit',cursor:start?'pointer':'default',textAlign:'left'}}>
      <div style={{fontSize:14}}>Started today</div><strong style={{fontSize:32}}>{clock(start)}</strong>
      <div style={{fontSize:12,opacity:.7}}>Dashboard default: started → now</div>
    </button>
    <div><div style={{fontSize:14}}>Span target finish</div><strong style={{fontSize:32}}>{clock(finish)}</strong></div>
    <a href="/d/rsynapse-workday/rsynapse-workday">Workday targets and daily log</a>
  </div>;
}
