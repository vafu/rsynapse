import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {chromium} from 'playwright';

test('workday settings save, preserve drafts on SSE/errors, and keep range controls',async()=>{
  let targets={span_hours:8,unlocked_hours:6,active_hours:4};
  const subscribers=new Set();const requests=[];
  const notify=()=>{for(const res of subscribers)res.write('event: workday-targets-changed\ndata: refresh\n\n');};
  const server=createServer(async(req,res)=>{
    const url=new URL(req.url,'http://localhost');
    const send=(body,status=200)=>{res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(body));};
    if(url.pathname==='/api/events') {
      res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('event: connected\ndata: ready\n\n');
      subscribers.add(res);res.on('close',()=>subscribers.delete(res));return;
    }
    if(url.pathname.startsWith('/api/'))requests.push([req.method,url.pathname]);
    if(url.pathname==='/api/workday-targets') {
      if(req.method==='GET')return send(targets);
      let body='';for await(const chunk of req)body+=chunk;
      const next=JSON.parse(body);
      if(next.active_hours>next.unlocked_hours)return send({error:'Active hours cannot exceed unlocked hours'},400);
      targets=next;notify();return send(targets);
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
    await page.getByRole('button',{name:'Edit workday targets'}).click();
    await page.getByLabel('Unlocked hours',{exact:true}).fill('5.5');
    targets={...targets,span_hours:9};notify();
    await page.getByText('9h span · 6h unlocked · 4h active').waitFor();
    assert.equal(await page.getByLabel('Unlocked hours',{exact:true}).inputValue(),'5.5');
    await page.getByRole('button',{name:'Save workday targets'}).click();
    await page.getByText('8h span · 5.5h unlocked · 4h active').waitFor();
    assert.equal(targets.unlocked_hours,5.5);
    await page.getByRole('button',{name:'Edit workday targets'}).click();
    await page.getByLabel('Active hours',{exact:true}).fill('7');
    await page.getByRole('button',{name:'Save workday targets'}).click();
    await page.getByRole('alert').filter({hasText:'Active hours cannot exceed unlocked hours'}).waitFor();
    assert.equal(await page.getByLabel('Active hours',{exact:true}).inputValue(),'7');
    assert.equal(targets.active_hours,4);
    await page.getByRole('button',{name:'Cancel targets'}).click();
    assert.ok(requests.every(([,path])=>path==='/api/workday-targets'));
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
