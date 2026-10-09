import { PanelPlugin } from '@grafana/data';
import { GoalsPanel } from './GoalsPanel';
export const plugin = new PanelPlugin(GoalsPanel).setPanelOptions(builder => builder.addTextInput({
  path: 'apiUrl', name: 'Local goal API', description: 'Loopback dashboard API on the desktop session host.', defaultValue: 'http://127.0.0.1:8770',
}));
