import type {CSSProperties} from 'react';
export type Identity = {
  instance_id:string; key:string; goal:{id:string;title:string;status:string;parent_id?:string;project_id?:string};
  project_name:string; created_at_ms:number; observed_at_ms:number; deleted_at_ms:number; ancestor_ids:string[];
};
export type Work = {
  id:string;instance_id:string;goal_key:string;goal_title:string;project_id:string;project_name:string;ancestor_ids:string[];
  kind:'active'|'idle'|'agent';started_at_ms:number;ended_at_ms:number;duration_ms:number;open:boolean;end_reason:string;
};
export type Stats = {
  instance_id:string;key:string;title:string;status:string;project_id:string;project_name:string;
  active_ms:number;idle_ms:number;agent_ms:number;in_progress_ms:number;created_at_ms:number;observed_at_ms:number;
  first_started_at_ms:number;completed_at_ms:number;lead_time_ms:number;cycle_time_ms:number;
  completion_count:number;interrupted_sessions:number;history_partial:boolean;
};
export type GoalEvent = {sequence:number;at_ms:number;kind:string;instance_id:string;key:string;old_status:string;goal:{title:string;status:string;project_id:string}};
export type MetricsData = {
  from_ms:number;to_ms:number;queried_at_ms:number;projects:Array<{id:string;name:string}>;
  identities:Identity[];work:Work[];stats:Stats[];history:GoalEvent[];
};
export type Range = {from:number;to:number;live?:boolean};
export const duration = (ms:number) => {
  if (ms<1000) return ms ? '<1s' : '0s';
  const seconds=Math.floor(ms/1000);const hours=Math.floor(seconds/3600);const minutes=Math.floor(seconds/60)%60;
  return hours?`${hours}h ${minutes}m`:minutes?`${minutes}m ${seconds%60}s`:`${seconds}s`;
};
export const timestamp = (ms:number) => ms ? new Date(ms).toLocaleString() : 'Unknown';
export const palette = {active:'#73bf69',idle:'#ff9830',agent:'#5794f2',unknown:'#8e8e8e'};
export const control:CSSProperties = {color:'inherit',background:'transparent',border:'1px solid #8888',borderRadius:4,padding:'5px 8px',font:'inherit'};
export function breadcrumb(id:string,identities:Identity[]) {
  const goal=identities.find(g=>g.instance_id===id);
  if (!goal) return id;
  return [...goal.ancestor_ids.map(parent=>identities.find(g=>g.instance_id===parent)?.goal.title).filter(Boolean),goal.goal.title].join(' › ');
}
