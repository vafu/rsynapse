import React from 'react';
import {PanelPlugin, getFieldDisplayName} from '@grafana/data';
import {locationService} from '@grafana/runtime';
import {WorkdayPanel} from './WorkdayPanel';
import {initialRangeSearch, rangeParams} from './workday-range.mjs';

function Panel(props) {
  const values: Record<string,number> = {};
  for (const frame of props.data.series) {
    for (const field of frame.fields) {
      if (field.type !== 'number') continue;
      const name = getFieldDisplayName(field, frame, props.data.series);
      for (let index=field.values.length-1;index>=0;index--) {
        const value=field.values[index];
        if (value!==null && Number.isFinite(value)) {values[name]=value;break;}
      }
    }
  }
  const navigation = performance.getEntriesByType('navigation')[0];
  return <WorkdayPanel width={props.width} height={props.height} start={values['Started today']} finish={values['Span target finish'] ?? values['8-hour span finish']}
    initialSearch={initialRangeSearch(window.location.href,navigation?.name)} autoStartRange={props.options.autoStartRange}
    setRange={(start,automatic)=>{
      const change=rangeParams(start,automatic);
      const current=new URLSearchParams(locationService.getLocation().search);
      if (Object.entries(change).some(([key,value])=>current.get(key)!==(value??null))) locationService.partial(change,true);
    }}/>;
}
export const plugin = new PanelPlugin(Panel).setPanelOptions(builder=>builder.addBooleanSwitch({
  path:'autoStartRange', name:'Default dashboard range to started today → now', defaultValue:true,
  description:'On load when no explicit range was provided. Manual and bookmarked ranges take precedence.',
}));
