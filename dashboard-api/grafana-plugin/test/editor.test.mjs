import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {chromium} from 'playwright';

test('goal editor creates, edits, updates status, handles errors and deletes records',async()=>{
  const goals=new Map();
  const links=new Map();
  let targets={span_hours:8,unlocked_hours:6,active_hours:4};
  const server=createServer(async(req,res)=>{
    const url=new URL(req.url,'http://localhost');
    const send=(body,status=200)=>{res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(body));};
    if(url.pathname==='/api/events') {res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('event: connected\ndata: ready\n\n');return;}
    if(url.pathname==='/api/goal-days')return send({today:'2026-10-07',days:['2026-10-07']});
    if(url.pathname==='/api/projects')return send(['/project']);
    if(url.pathname==='/api/workday-targets'&&req.method==='GET')return send(targets);
    if(url.pathname==='/api/sessions')return send([{agent:'codex',session_id:'session-1',title:'Fixture session',state:'thinking',cwd:'/other'}]);
    if(req.method==='GET'&&url.pathname==='/api/goals')return send([...goals.values()].filter(g=>g.date===url.searchParams.get('date')));
    let body='';for await(const chunk of req)body+=chunk;
    if(url.pathname==='/api/workday-targets'&&req.method==='PUT'){targets=JSON.parse(body);return send(targets);}
    if(req.method==='POST'&&url.pathname==='/api/goals'){
      const goal=JSON.parse(body);if(goals.has(goal.id))return send({error:'Goal already exists'},409);
      goals.set(goal.id,goal);return send(goal,201);
    }
    const match=url.pathname.match(/^\/api\/goals\/[^/]+\/([^/]+)(\/(?:status|links))?$/);
    if(match){
      const goal=goals.get(match[1]);if(!goal)return send({error:'Goal not found'},404);
      if(match[2]==='/links') {
        const list=links.get(goal.id)||[];
        if(req.method==='GET')return send(list);
        const value=JSON.parse(body);
        links.set(goal.id,req.method==='POST'?[...list,value]:list.filter(x=>x.agent!==value.agent||x.session_id!==value.session_id));
        res.writeHead(204);res.end();return;
      }
      if(req.method==='PATCH'){Object.assign(goal,JSON.parse(body));return send(goal);}
      if(req.method==='DELETE'){goals.delete(match[1]);res.writeHead(204);res.end();return;}
    }
    if(url.pathname==='/'||url.pathname==='/preview.js'){
      const path=url.pathname==='/'?'test-build/index.html':'test-build/preview.js';
      res.writeHead(200,{'Content-Type':url.pathname==='/'?'text/html':'text/javascript'});res.end(await readFile(path));return;
    }
    res.writeHead(404);res.end();
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await chromium.launch({executablePath:process.env.CHROME_BIN||'/usr/bin/google-chrome',headless:true});
  const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  try {
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.getByLabel('Goal day',{exact:true}).fill('2026-10-07');
    await page.getByRole('button',{name:'Edit workday targets'}).click();
    await page.getByLabel('Unlocked hours',{exact:true}).fill('5.5');
    await page.getByRole('button',{name:'Save workday targets'}).click();
    await page.getByText('8h span · 5.5h unlocked · 4h active').waitFor();
    assert.equal(targets.unlocked_hours,5.5);
    await page.getByRole('button',{name:'New goal',exact:true}).click();
    await page.getByLabel('Goal ID',{exact:true}).fill('habit1');
    await page.getByLabel('Title',{exact:true}).fill('Protect focus');
    await page.getByLabel('Kind',{exact:true}).selectOption('habit');
    await page.getByLabel('Success criterion',{exact:true}).fill('Two focused blocks');
    await page.getByRole('button',{name:'Create goal',exact:true}).click();
    await page.waitForFunction(()=>document.body.textContent.includes('habit1')&&!document.querySelector('form'));
    assert.equal(goals.get('habit1').success,'Two focused blocks');
    assert.equal(goals.get('habit1').project,null);
    await page.getByLabel('Status for habit1',{exact:true}).selectOption('completed');
    await page.waitForFunction(()=>document.body.textContent.includes('1 / 1 completed'));
    assert.equal(goals.get('habit1').status,'completed');
    await page.getByRole('button',{name:'Edit',exact:true}).click();
    await page.getByLabel('Title',{exact:true}).fill('Protected focus updated');
    await page.getByRole('button',{name:'Save changes',exact:true}).click();
    await page.waitForFunction(()=>!document.querySelector('form')&&document.body.textContent.includes('Protected focus updated'));
    assert.equal(goals.get('habit1').status,'completed');
    await page.getByRole('button',{name:'New goal',exact:true}).click();
    await page.getByLabel('Goal ID',{exact:true}).fill('habit1');
    await page.getByLabel('Title',{exact:true}).fill('Duplicate');
    await page.getByLabel('Kind',{exact:true}).selectOption('habit');
    await page.getByLabel('Success criterion',{exact:true}).fill('Criterion');
    await page.getByRole('button',{name:'Create goal',exact:true}).click();
    await page.getByRole('alert').filter({hasText:'Goal already exists'}).waitFor();
    await page.getByRole('button',{name:'Cancel',exact:true}).click();
    await page.getByLabel('Delete habit1',{exact:true}).click();
    await page.getByText('No goals for this day. Add an outcome or a project-independent habit.').waitFor();
    assert.equal(goals.size,0);assert.deepEqual(errors,[]);
    await page.getByRole('button',{name:'New goal',exact:true}).click();
    await page.getByLabel('Goal ID',{exact:true}).fill('outcome1');
    await page.getByLabel('Title',{exact:true}).fill('Ship outcome');
    await page.getByLabel('Project',{exact:true}).fill('/project');
    await page.getByLabel('Success criterion',{exact:true}).fill('Observable result');
    await page.getByRole('button',{name:'Create goal',exact:true}).click();
    await page.getByRole('button',{name:'Sessions',exact:true}).click();
    await page.getByLabel('Agent session',{exact:true}).selectOption('0');
    await page.getByRole('button',{name:'Link session',exact:true}).click();
    await page.getByRole('button',{name:'Unlink',exact:true}).waitFor();
    assert.equal(links.get('outcome1')[0].session_id,'session-1');
    await page.getByRole('button',{name:'Unlink',exact:true}).click();
    await page.waitForFunction(()=>!Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='Unlink'));
    assert.deepEqual(links.get('outcome1'),[]);
    assert.deepEqual(errors,[]);
    await page.goto(`http://127.0.0.1:${server.address().port}/?mode=workday&var-idle=false`);
    await page.waitForFunction(()=>new URL(location.href).searchParams.get('from')==='1791390000000');
    let range=new URL(page.url()).searchParams;
    assert.equal(range.get('to'),'now');assert.equal(range.get('var-idle'),'false');
    assert.equal(range.get('rsynapseWorkdayStart'),'1791390000000');
    await page.evaluate(()=>{const url=new URL(location.href);url.searchParams.set('from','now-2h');history.replaceState({},'',url);});
    await page.getByRole('button',{name:'Simulate data refresh'}).click();
    assert.equal(new URL(page.url()).searchParams.get('from'),'now-2h');
    assert.equal(await page.evaluate(()=>window.rangeChanges),1);
    await page.goto(`http://127.0.0.1:${server.address().port}/?mode=workday&from=now-7d&to=now`);
    await page.getByRole('button',{name:'Simulate data refresh'}).click();
    assert.equal(new URL(page.url()).searchParams.get('from'),'now-7d');
    assert.equal(await page.evaluate(()=>window.rangeChanges||0),0);
    assert.deepEqual(errors,[]);
  } finally {await browser.close();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});
