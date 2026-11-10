import {PanelPlugin} from '@grafana/data';
import {WorkdayTargetsPanel} from './WorkdayTargetsPanel';
export const plugin = new PanelPlugin(WorkdayTargetsPanel).setPanelOptions(builder=>builder.addTextInput({
  path:'apiUrl',name:'Local dashboard API',description:'Loopback dashboard API on the desktop session host.',defaultValue:'http://127.0.0.1:8770',
}));
