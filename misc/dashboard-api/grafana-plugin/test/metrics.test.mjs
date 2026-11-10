import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {chromium} from 'playwright';

test('goal metrics filters timeline/lifetimes, avoids rollup double counting and refreshes via SSE/range',async()=>{
  const from=1791900000000,to=from+86400000;
  const goal=(id,title,parent=[],project='p')=>({instance_id:id,key:id,goal:{id,title,status:'in-progress',project_id:project},project_name:project==='p'?'Project A':'Project B',created_at_ms:0,observed_at_ms:from,deleted_at_ms:0,ancestor_ids:parent});
  const identities=[goal('root','Parent objective'),goal('child','Concrete step',['root']),{...goal('old','Old step',['root']),deleted_at_ms:from+10000},goal('other','Other task',[],'q')];
  let active=60000;
  const span=(id,instance,kind,start,duration,reason='changed')=>({id,instance_id:instance,goal_key:instance,goal_title:identities.find(g=>g.instance_id===instance).goal.title,project_id:instance==='other'?'q':'p',project_name:instance==='other'?'Project B':'Project A',ancestor_ids:instance==='child'?['root']:[],kind,started_at_ms:from+start,ended_at_ms:from+start+60000,duration_ms:duration,open:false,end_reason:reason});
  const events=[{sequence:1,at_ms:from+120000,kind:'status-changed',instance_id:'child',key:'child',old_status:'in-progress',goal:{title:'Concrete step',status:'completed',project_id:'p'}},{sequence:2,at_ms:from+130000,kind:'deleted',instance_id:'old',key:'old',old_status:'completed',goal:{title:'Old step',status:'completed',project_id:'p'}}];
  const subscribers=new Set();const queries=[];const methods=[];let fail=false;
  const server=createServer(async(req,res)=>{
    const url=new URL(req.url,'http://localhost');methods.push(req.method);
    const send=(body,status=200)=>{res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(body));};
    if(url.pathname==='/api/events'){res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('event: connected\ndata: ready\n\n');subscribers.add(res);req.on('close',()=>subscribers.delete(res));return;}
    if(url.pathname==='/api/goal-metrics'){
      queries.push(Object.fromEntries(url.searchParams));if(fail)return send({error:'Recorder unavailable'},503);
      const p=url.searchParams.get('project_id'),g=url.searchParams.get('instance_id'),include=url.searchParams.get('include_subgoals')==='true';
      const start=Number(url.searchParams.get('from_ms')),end=Number(url.searchParams.get('to_ms'));
      const work=[span('a','child','active',1000,active),span('i','child','idle',62000,60000),span('agent','child','agent',123000,30000),span('gap','child','active',185000,0,'recorder-restarted-unknown-duration'),span('b','other','active',1000,120000)]
        .filter(w=>(!p||w.project_id===p)&&(!g||w.instance_id===g||(include&&w.ancestor_ids.includes(g)))&&w.started_at_ms>=start&&w.ended_at_ms<=end);
      const selected=identities.filter(x=>(!p||x.goal.project_id===p)&&(!g||x.instance_id===g||(include&&x.ancestor_ids.includes(g))));
      const stats=selected.map(x=>{
        const spans=work.filter(w=>w.instance_id===x.instance_id||(include&&w.ancestor_ids.includes(x.instance_id)));
        return {instance_id:x.instance_id,key:x.key,title:x.goal.title,status:x.goal.status,project_id:x.goal.project_id,project_name:x.project_name,active_ms:spans.filter(w=>w.kind==='active').reduce((n,w)=>n+w.duration_ms,0),idle_ms:spans.filter(w=>w.kind==='idle').reduce((n,w)=>n+w.duration_ms,0),agent_ms:spans.filter(w=>w.kind==='agent').reduce((n,w)=>n+w.duration_ms,0),in_progress_ms:150000,created_at_ms:0,observed_at_ms:from,first_started_at_ms:0,completed_at_ms:x.instance_id==='child'?from+120000:0,lead_time_ms:0,cycle_time_ms:0,completion_count:1,interrupted_sessions:spans.some(w=>w.id==='gap')?1:0,history_partial:true};
      });
      return send({from_ms:start,to_ms:end,queried_at_ms:from+200000,projects:[{id:'p',name:'Project A'},{id:'q',name:'Project B'}],identities,stats,work,history:events.filter(e=>(!p||e.goal.project_id===p)&&selected.some(x=>x.instance_id===e.instance_id)&&e.at_ms>=start&&e.at_ms<end)});
    }
    if(url.pathname==='/'||url.pathname==='/preview.js'){res.writeHead(200,{'Content-Type':url.pathname==='/'?'text/html':'text/javascript'});res.end(await readFile(url.pathname==='/'?'test-build/index.html':'test-build/preview.js'));return;}
    res.writeHead(404);res.end();
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await chromium.launch({executablePath:process.env.CHROME_BIN||'/usr/bin/google-chrome',headless:true});
  const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  try{
    await page.goto(`http://127.0.0.1:${server.address().port}/?mode=metrics`);
    await page.getByRole('heading',{name:'When you worked on which goal'}).waitFor();
    await page.getByLabel('Metrics project',{exact:true}).selectOption('p');
    const totals=page.getByRole('region',{name:'Goal metric totals'});
    const activeValue=totals.locator('div').filter({has:page.getByText('Input-active focus',{exact:true})}).locator('strong');
    await activeValue.filter({hasText:'1m 0s'}).waitFor();
    assert.equal(await page.locator('rect[data-work-kind="active"]').count(),1);
    assert.equal(await page.locator('rect[data-work-kind="unknown"]').count(),1);
    assert.ok(await page.getByText('Unknown',{exact:true}).count()>0);
    await page.getByLabel('Metrics goal',{exact:true}).selectOption('root');
    await page.getByRole('button',{name:'Concrete step',exact:true}).first().waitFor();
    await page.getByLabel('Include subgoals',{exact:true}).uncheck();
    await page.getByText('No work intervals recorded in this range.').waitFor();
    await page.getByLabel('Include subgoals',{exact:true}).check();
    await page.locator('rect[data-work-kind="active"]').waitFor();
    active=90000;for(const res of subscribers)res.write('event: goal-metrics-changed\ndata: refresh\n\n');
    await activeValue.filter({hasText:'1m 30s'}).waitFor();
    await page.getByLabel('Metrics goal',{exact:true}).selectOption('old');
    await page.getByText('Removed task instance',{exact:true}).waitFor();
    await page.getByLabel('Metrics goal',{exact:true}).selectOption('');
    await page.getByRole('button',{name:'Change dashboard range'}).click();
    await page.getByText('No work intervals recorded in this range.').waitFor();
    assert.equal(queries.at(-1).from_ms,String(from+3600000));
    fail=true;await page.getByRole('button',{name:'Refresh metrics',exact:true}).click();
    await page.getByRole('alert').filter({hasText:'Recorder unavailable'}).waitFor();
    assert.equal(await page.getByRole('region',{name:'Goal metric totals'}).count(),0);
    assert.deepEqual(errors,[]);assert.ok(methods.every(method=>method==='GET'));
  }finally{await browser.close();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});
