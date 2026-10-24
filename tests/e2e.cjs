// Optional browser acceptance test. Requires Playwright and its Chromium.
const {chromium}=require('playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs/promises');
const path=require('node:path');
const os=require('node:os');
const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..');
async function waitText(page,id,text){for(let i=0;i<250;i++){if((await page.locator('#'+id).textContent()).includes(text))return;await new Promise(r=>setTimeout(r,25));}throw Error('Timed out: '+id+' '+text+' actual: '+await page.locator('#'+id).textContent());}
const row=(page,id)=>page.locator('#tasks tr[data-id="'+id+'"]');
async function clickDone(page,selector,text){await page.click(selector);await waitText(page,'status',text);}
(async()=>{
 const server=spawn(path.join(root,'target','release',process.platform==='win32'?'forgeplan.exe':'forgeplan'),['serve','8787'],{stdio:'ignore'});
 let browser;
 try{
  browser=await chromium.launch({headless:true,args:['--no-sandbox']});
  const page=await browser.newPage({viewport:{width:1550,height:1050}});const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('dialog',d=>d.accept());
  await page.goto('http://127.0.0.1:8787');await waitText(page,'status','Проект загружен');
  assert.equal(await page.locator('#tasks tr').count(),8);assert.equal(await page.locator('.summary-bar').count(),2);
  assert.equal(await row(page,10).locator('[data-key="duration"]').isDisabled(),true);
  await row(page,2).locator('[data-key="duration"]').fill('3840');
  await clickDone(page,'#apply','Изменения применены');assert.match(await page.locator('#summary').innerText(),/136/);
  await clickDone(page,'#undo','Команда отменена');assert.equal(await row(page,2).locator('[data-key="duration"]').inputValue(),'2400');
  await clickDone(page,'#redo','Команда повторена');assert.equal(await row(page,2).locator('[data-key="duration"]').inputValue(),'3840');
  await row(page,1).locator('[data-key="links"]').fill('6:FS:0');await clickDone(page,'#apply','cycle');
  assert.equal(await page.locator('#undo').isDisabled(),true);await page.click('#discard');assert.equal(await row(page,1).locator('[data-key="links"]').inputValue(),'');
  await row(page,3).locator('[data-key="parent"]').selectOption('20');await clickDone(page,'#apply','Изменения применены');assert.equal(await row(page,3).locator('td').nth(1).textContent(),'2.4');
  await clickDone(page,'#undo','Команда отменена');assert.equal(await row(page,3).locator('td').nth(1).textContent(),'1.3');
  await row(page,10).getByRole('button',{name:'Удалить поддерево 10',exact:true}).click();await waitText(page,'status','Изменения применены');assert.equal(await page.locator('#tasks tr').count(),4);
  await clickDone(page,'#undo','Команда отменена');assert.equal(await page.locator('#tasks tr').count(),8);
  await clickDone(page,'#phase','Изменения применены');const phaseId=Number((await page.locator('#tasks tr').last().getAttribute('data-id')));
  await row(page,phaseId).getByRole('button',{name:'Добавить задачу в этап '+phaseId,exact:true}).click();await waitText(page,'status','Изменения применены');assert.equal(await page.locator('#tasks tr').count(),10);
  const childId=Number((await page.locator('#tasks tr').last().getAttribute('data-id')));
  await row(page,childId).getByRole('button',{name:'Поднять уровень '+childId,exact:true}).click();await waitText(page,'status','Изменения применены');assert.equal(await row(page,childId).locator('[data-key="parent"]').inputValue(),'-');
  // Change a label that looks like HTML. It must stay plain text in both views.
  await row(page,1).locator('[data-key="title"]').fill('<img src=x onerror=alert(1)>');await clickDone(page,'#apply','Изменения применены');assert.equal(await page.locator('#output img').count(),0);
  const downloadPromise=page.waitForEvent('download');await page.click('#save');const download=await downloadPromise;const saved=await fs.readFile(await download.path(),'utf8');assert.equal(JSON.parse(saved).schema_version,1);assert.equal(JSON.parse(saved).project.tasks.find(t=>t.id===2).duration,3840);
  const legacy=await fs.readFile(path.join(root,'examples/legacy-v1.fplan'));await page.locator('#open').setInputFiles({name:'legacy.fplan',mimeType:'text/plain',buffer:legacy});await waitText(page,'status','Проект открыт');assert.equal(await page.locator('#tasks tr').count(),6);
  // Undo import is a domain command too.
  await clickDone(page,'#undo','Команда отменена');assert.equal(await page.locator('#tasks tr').count(),10);
  await page.locator('#open').setInputFiles({name:'saved.fplan',mimeType:'text/plain',buffer:Buffer.from(saved)});await waitText(page,'status','Проект открыт');assert.equal(await row(page,2).locator('[data-key="duration"]').inputValue(),'3840');
  await page.locator('#open').setInputFiles({name:'bad.fplan',mimeType:'text/plain',buffer:Buffer.from('forgeplan\t99\n')});await waitText(page,'status','Expected forgeplan');assert.equal(await page.locator('#tasks tr').count(),10);
  // A second tab must get 409 rather than overwrite the first tab's revision.
  const second=await browser.newPage();second.on('dialog',d=>d.accept());await second.goto('http://127.0.0.1:8787');await waitText(second,'status','Проект загружен');
  await row(page,2).locator('[data-key="duration"]').fill('4320');await clickDone(page,'#apply','Изменения применены');
  await row(second,2).locator('[data-key="duration"]').fill('4800');await clickDone(second,'#apply','Revision conflict');assert.equal(await row(second,2).locator('[data-key="duration"]').inputValue(),'4800');
  await second.click('#discard');await clickDone(second,'#refresh','Состояние загружено');assert.equal(await row(second,2).locator('[data-key="duration"]').inputValue(),'4320');await second.close();
  // Minute calendar fields, constraints and deadline diagnostics.
  await row(page,3).locator('[data-key="constraint_type"]').selectOption('SNET');
  await row(page,3).locator('[data-key="constraint_at"]').fill('2026-10-30T10:15');
  await row(page,3).locator('[data-key="deadline"]').fill('2026-10-30T11:00');
  await clickDone(page,'#apply','Изменения применены');
  let response=await page.request.get('http://127.0.0.1:8787/api/state');let snapshot=await response.json();let networkTask=snapshot.schedule.tasks.find(t=>t.id===3);
  assert.equal(networkTask.start,'2026-10-30T10:15+03:00');assert.equal(networkTask.deadline_missed,true);assert.match(await page.locator('#output').innerText(),/Дедлайн превышен/);
  await clickDone(page,'#undo','Команда отменена');assert.equal(await row(page,3).locator('[data-key="constraint_type"]').inputValue(),'-');
  await row(page,2).locator('[data-key="constraint_type"]').selectOption('MSO');await row(page,2).locator('[data-key="constraint_at"]').fill('2026-11-01T10:00');await clickDone(page,'#apply','outside a working');await page.click('#discard');
  await page.locator('#weekly').fill(['0','1','2','3','4'].map(d=>d+'=09:00-13:00,14:00-18:00').concat(['5=-','6=-']).join('\n'));
  await page.locator('#start-time').fill('13:15');await page.locator('#exceptions').fill('2026-10-31=10:00-14:00');await clickDone(page,'#apply','Изменения применены');
  response=await page.request.get('http://127.0.0.1:8787/api/state');snapshot=await response.json();assert.equal(snapshot.schedule.tasks.find(t=>t.id===1).start,'2026-10-26T14:00+03:00');
  assert.equal(JSON.parse(snapshot.document).project.calendar.weekly[0].length,2);await clickDone(page,'#undo','Команда отменена');
  // Resource editor, fractional assignments, absence, delete/undo and report views.
  assert.equal(await page.locator('#resources tr').count(),3);
  assert.ok(await page.locator('#resource-summary .resource-overloaded').count()>0);
  await row(page,3).locator('[data-key="assignments"]').fill('1:50, 2:100');await clickDone(page,'#apply','Изменения применены');
  response=await page.request.get('http://127.0.0.1:8787/api/state');snapshot=await response.json();assert.equal(snapshot.schedule.tasks.find(t=>t.id===3).work_minutes,2160);
  await row(page,3).locator('[data-key="assignments"]').fill('999:100');await clickDone(page,'#apply','Unknown resource');await page.click('#discard');
  const resourceRow=id=>page.locator('#resources tr[data-id="'+id+'"]');
  await resourceRow(1).locator('[data-resource-key="periods"]').fill('2026-10-26..2026-12-31:0');await clickDone(page,'#apply','Изменения применены');
  response=await page.request.get('http://127.0.0.1:8787/api/state');snapshot=await response.json();let load=snapshot.schedule.resource_report.resources.find(r=>r.id===1);assert.equal(load.capacity_minutes,0);assert.ok(load.overloaded_minutes>0);await clickDone(page,'#undo','Команда отменена');
  await resourceRow(1).getByRole('button',{name:'Удалить ресурс 1',exact:true}).click();await waitText(page,'status','Изменения применены');assert.equal(await page.locator('#resources tr').count(),2);
  response=await page.request.get('http://127.0.0.1:8787/api/state');snapshot=await response.json();assert.equal(snapshot.schedule.resource_report.assignments.some(a=>a.resource===1),false);await clickDone(page,'#undo','Команда отменена');assert.equal(await page.locator('#resources tr').count(),3);
  await clickDone(page,'#resource-add','Изменения применены');const newResource=Number(await page.locator('#resources tr').last().getAttribute('data-id'));
  await resourceRow(newResource).locator('[data-resource-key="name"]').fill('QA team');await resourceRow(newResource).locator('[data-resource-key="kind"]').selectOption('generic');await resourceRow(newResource).locator('[data-resource-key="capacity"]').fill('50');
  await row(page,3).locator('[data-key="assignments"]').fill(newResource+':25.5');await clickDone(page,'#apply','Изменения применены');response=await page.request.get('http://127.0.0.1:8787/api/state');snapshot=await response.json();assert.equal(snapshot.schedule.tasks.find(t=>t.id===3).work_minutes,367.2);assert.equal(snapshot.schedule.resource_report.assignments.find(a=>a.task===3).units_bp,2550);
  await clickDone(page,'#undo','Команда отменена');await clickDone(page,'#undo','Команда отменена');assert.equal(await page.locator('#resources tr').count(),3);
  await row(page,3).locator('[data-key="assignments"]').fill('1:33.333');await clickDone(page,'#apply','не более двух');await page.click('#discard');
  await page.locator('#load-resource').selectOption('1');assert.ok(await page.locator('#resource-overloads tr').count()>0);assert.ok(await page.locator('#resource-daily tr').count()>0);
  await page.getByText('Дневная загрузка',{exact:true}).click();assert.equal(await page.locator('#resource-daily').isVisible(),true);await page.getByText('Дневная загрузка',{exact:true}).click();
  // Collapse changes the view only; project content is not changed.
  await row(page,10).getByRole('button',{name:'Свернуть или развернуть этап 10',exact:true}).click();assert.equal(await row(page,1).isVisible(),false);await row(page,10).getByRole('button',{name:'Свернуть или развернуть этап 10',exact:true}).click();assert.equal(await row(page,1).isVisible(),true);
  await page.screenshot({path:path.join(os.tmpdir(),'forgeplan-v04-desktop.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});assert.equal(Math.round((await page.locator('html').boundingBox()).width),390);await page.screenshot({path:path.join(os.tmpdir(),'forgeplan-v04-mobile.png'),fullPage:true});
  assert.deepEqual(errors,[]);console.log('Browser E2E passed: resource/availability/assignment/load/undo plus minute calendars and WBS, summary rollups, apply/undo/redo, move, subtree delete, child creation/outdent, save/import/migration, failed import, concurrent tabs, collapse, mobile layout, text escaping.');
 }finally{if(browser)await browser.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
