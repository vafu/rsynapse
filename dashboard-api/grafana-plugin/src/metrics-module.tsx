import React,{useEffect,useState} from 'react';
import {PanelPlugin} from '@grafana/data';
import {RefreshEvent,getAppEvents} from '@grafana/runtime';
import {GoalMetricsPanel} from './GoalMetricsPanel';

function Panel(props) {
  const [refresh,setRefresh]=useState(0);
  useEffect(()=>{const sub=getAppEvents().getStream(RefreshEvent).subscribe(()=>setRefresh(n=>n+1));return()=>sub.unsubscribe();},[]);
  return <GoalMetricsPanel width={props.width} height={props.height} options={props.options} refreshKey={refresh}
    range={{from:props.timeRange.from.valueOf(),to:props.timeRange.to.valueOf(),live:props.timeRange.raw.to==='now'}}/>;
}
export const plugin=new PanelPlugin(Panel).setPanelOptions(builder=>builder.addTextInput({
  path:'apiUrl',name:'Local goal metrics API',description:'Loopback dashboard-api adapter over projd Metrics1.',defaultValue:'http://127.0.0.1:8770',
}));
