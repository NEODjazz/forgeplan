const nativeFetch=window.fetch.bind(window);
let activeProject=new URLSearchParams(location.search).get('project')||'demo';
function fpFetch(url,options={}){const headers={...(options.headers||{}),'X-Organization':sessionStorage.getItem('fp-org')||'local','X-Project-ID':activeProject};const token=sessionStorage.getItem('fp-token');if(token)headers.Authorization='Bearer '+token;if(options.method==='POST')headers['X-Command-ID']=crypto.randomUUID();return nativeFetch(url,{...options,headers});}
'use strict';
const $=id=>document.getElementById(id);
let state=null,busy=false,dirty=false,savedDocument=null,userRole='admin';
const collapsed=new Set();
function node(tag,text,cls){const e=document.createElement(tag);if(text!==undefined)e.textContent=text;if(cls)e.className=cls;return e;}
function status(text,error=false){$('status').textContent=text;$('status').className='status'+(error?' error':'');}
function clean(s){if(/[\t\r\n]/.test(s))throw Error('Табуляции и переводы строк в полях не поддерживаются');return s;}
function syncButtons(){
 const blocked=busy||userRole==='viewer';$('apply').disabled=blocked; $('save').disabled=busy; $('open').disabled=busy;
 $('undo').disabled=blocked||dirty||!state?.can_undo;$('redo').disabled=blocked||dirty||!state?.can_redo;
 $('discard').disabled=busy||!dirty;
 for(const e of document.querySelectorAll('#tasks input,#tasks select,#tasks button,#resources input,#resources select,#resources button,#resource-add,#management input,#management select,#baseline-name,#status-date,#title,#start,#holidays,#start-time,#offset,#weekly,#exceptions,#add,#phase'))e.disabled=blocked||e.dataset.readonly==='true';
 $('revision').textContent=state?'Ревизия '+state.revision:'Загрузка…';
}
function changed(){dirty=true;status('Есть неприменённые изменения. Примените их или отмените ввод.');$('status').className='status stale';syncButtons();}
function taskById(id){return state.schedule.tasks.find(t=>t.id===id);}
function children(parent){return state.schedule.tasks.filter(t=>t.parent===parent);}
function descendants(id){const ids=new Set([id]);let previous;do{previous=ids.size;for(const t of state.schedule.tasks)if(ids.has(t.parent))ids.add(t.id);}while(previous!==ids.size);return ids;}
function isHidden(t){let parent=t.parent;while(parent!==null){if(collapsed.has(parent))return true;parent=taskById(parent).parent;}return false;}
function button(text,title,action){const b=node('button',text);b.title=title;b.setAttribute('aria-label',title);b.onclick=action;return b;}
function input(key,value,type,id){const e=node('input');e.dataset.key=key;e.value=value;e.type=type;e.setAttribute('aria-label',key+' задачи '+id);if(type==='number'){e.min='0';e.max='48000000';e.step='1';}e.oninput=changed;return e;}
function documentSettings(document){const p=JSON.parse(document).project;return{title:p.title,start:p.calendar.start,holidays:p.calendar.holidays};}

function renderEditor(){
 const settings=documentSettings(state.document);$('title').value=settings.title;$('start').value=settings.start;$('holidays').value=settings.holidays.join(', ');const cal=state.schedule.calendar;$('start-time').value=cal.start_time;$('offset').value=cal.offset;$('weekly').value=cal.weekly.map((hours,i)=>i+'='+hours).join('\n');$('exceptions').value=cal.exceptions.map(e=>e.date+'='+e.hours).join('\n');
 $('tasks').replaceChildren();
 const rows=state.schedule.tasks;
 for(const t of rows){
  const tr=node('tr',undefined,t.kind==='summary'?'summary-row':'');tr.dataset.id=t.id;if(isHidden(t))tr.hidden=true;
  tr.append(node('td',String(t.id)),node('td',t.wbs));
  const name=node('td'),wrap=node('div',undefined,'task-name');wrap.style.paddingLeft=Math.min(200,t.depth*18)+'px';
  if(t.kind==='summary')wrap.append(button(collapsed.has(t.id)?'▸':'▾','Свернуть или развернуть этап '+t.id,()=>toggle(t.id)));
  wrap.append(input('title',t.title,'text',t.id));name.append(wrap);tr.append(name);
  const duration=node('td');const value=input('duration',t.duration,'number',t.id);
  if(t.kind==='summary'){value.dataset.readonly='true';value.disabled=true;value.title='Длительность вычисляется по дочерним задачам';}duration.append(value);tr.append(duration);
  const parentTd=node('td'),select=node('select');select.dataset.key='parent';select.setAttribute('aria-label','Родитель задачи '+t.id);
  const forbidden=descendants(t.id);for(const p of [{id:null,title:'Корень'},...rows.filter(p=>p.kind==='summary'&&!forbidden.has(p.id))]){
   const option=node('option',p.id===null?p.title:p.wbs+' · '+p.title);option.value=p.id===null?'-':String(p.id);select.append(option);
  }select.value=t.parent===null?'-':String(t.parent);select.onchange=changed;parentTd.append(select);tr.append(parentTd);
  const pred=node('td');const links=parseLinks(state.document).filter(l=>l.to===t.id).map(l=>l.from+':'+l.type+':'+l.lag).join(', ');
  const dep=input('links',links,'text',t.id);if(t.kind==='summary'){dep.dataset.readonly='true';dep.disabled=true;dep.placeholder='Связи только у задач';}pred.append(dep);tr.append(pred);
  const dateTd=node('td'),type=node('select');type.dataset.key='constraint_type';type.setAttribute('aria-label','Ограничение задачи '+t.id);
  for(const [key,label]of[['-','Без ограничения'],['SNET','Начало не ранее'],['SNLT','Начало не позднее'],['FNET','Окончание не ранее'],['FNLT','Окончание не позднее'],['MSO','Начать точно'],['MFO','Закончить точно']]){const option=node('option',label);option.value=key;type.append(option);}type.value=t.constraint_type??'-';type.onchange=changed;
  const constraintAt=input('constraint_at',t.constraint_at??'','datetime-local',t.id);constraintAt.step='60';dateTd.append(type,constraintAt);tr.append(dateTd);
  const deadlineTd=node('td'),deadline=input('deadline',t.deadline??'','datetime-local',t.id);deadline.step='60';deadlineTd.append(deadline);tr.append(deadlineTd);
  if(t.kind==='summary')for(const e of[type,constraintAt,deadline]){e.dataset.readonly='true';e.disabled=true;}
  const assignmentTd=node('td');const entries=state.schedule.resource_report.assignments.filter(a=>a.task===t.id).map(a=>a.resource+':'+(a.units_bp/100)).join(', ');const assigned=input('assignments',entries,'text',t.id);assigned.placeholder='1:100, 2:50';assigned.title='ID ресурса:процент назначения';if(t.kind==='summary'){assigned.dataset.readonly='true';assigned.disabled=true;}assignmentTd.append(assigned);tr.append(assignmentTd);
  const actions=node('td',undefined,'row-actions');const siblings=children(t.parent);const at=siblings.findIndex(s=>s.id===t.id);
  const up=button('↑','Выше '+t.id,()=>structural(['move\t'+t.id+'\t'+(t.parent??'-')+'\t'+siblings[at-1].id]));up.dataset.readonly=String(at===0);up.disabled=at===0;
  const down=button('↓','Ниже '+t.id,()=>structural(['move\t'+t.id+'\t'+(t.parent??'-')+'\t'+(siblings[at+2]?.id??'-')]));down.dataset.readonly=String(at===siblings.length-1);down.disabled=at===siblings.length-1;
  actions.append(up,down);
  const indent=button('→','Вложить в предыдущий этап '+t.id,()=>structural(['move\t'+t.id+'\t'+siblings[at-1].id+'\t-']));const canIndent=at>0&&siblings[at-1].kind==='summary';indent.dataset.readonly=String(!canIndent);indent.disabled=!canIndent;actions.append(indent);
  const outdent=button('←','Поднять уровень '+t.id,()=>{const parent=taskById(t.parent),peers=children(parent.parent),pos=peers.findIndex(p=>p.id===parent.id);structural(['move\t'+t.id+'\t'+(parent.parent??'-')+'\t'+(peers[pos+1]?.id??'-')]);});outdent.dataset.readonly=String(t.parent===null);outdent.disabled=t.parent===null;actions.append(outdent);
  if(t.kind==='summary')actions.append(button('+','Добавить задачу в этап '+t.id,()=>addTask('task',t.id)));
  actions.append(button('×','Удалить поддерево '+t.id,()=>structural(['delete\t'+t.id])));tr.append(actions);$('tasks').append(tr);
 }
}
function parseLinks(document){return JSON.parse(document).project.links.map(l=>({from:l.from,to:l.to,type:l.relation,lag:l.lag}));}

function displayTime(iso){return iso.replace('T',' ').replace(/([+-]\d\d:\d\d)$/,' $1');}
function renderSchedule(){
 const data=state.schedule;const leaves=data.tasks.filter(t=>t.kind==='task');$('summary').replaceChildren();
 for(const [value,label]of [[leaves.length,'задач и вех'],[data.tasks.length-leaves.length,'этапов WBS'],[Number((data.working_minutes/60).toFixed(2)),'длительность, раб. ч.'],[leaves.filter(t=>t.critical).length,'критических задач и вех']]){const e=node('div',undefined,'metric');e.append(node('strong',String(value)),node('span',label));$('summary').append(e);}
 const chart=$('gantt');chart.replaceChildren();const axis=node('div',undefined,'axis'),ticks=node('div',undefined,'ticks');axis.append(node('span','WBS · рабочие минуты'));const horizon=Math.max(1,data.working_minutes);
 for(let i=0;i<=5;i++)ticks.append(node('span',String(Number((horizon*i/5).toFixed(1)))));axis.append(ticks);chart.append(axis);$('output').replaceChildren();
 for(const t of data.tasks){
  if(isHidden(t))continue;
  const row=node('div',undefined,'gantt-row');const label=node('div',t.wbs+' · '+t.title,'task-label');label.style.paddingLeft=Math.min(140,t.depth*14)+'px';label.title=t.title;
  const track=node('div',undefined,'track'),bar=node('div',undefined,'bar'+(t.kind==='summary'?' summary-bar':t.critical?' critical':'')+(t.duration===0&&t.kind!=='summary'?' milestone':''));bar.style.left=(100*t.es/horizon)+'%';bar.style.width=(100*t.duration/horizon)+'%';bar.title=t.title+' · '+t.start+' → '+t.finish;track.append(bar);row.append(label,track);chart.append(row);
  const tr=node('tr',undefined,t.kind==='summary'?'summary-row':'');for(const value of[t.wbs,t.title,displayTime(t.start),displayTime(t.finish),Number((t.work_minutes/60).toFixed(2)),t.float??'—',t.kind==='summary'?'Этап':t.critical?'Да':'Нет'])tr.append(node('td',String(value)));if(t.critical)tr.lastChild.className='yes';const why=node('td'),details=node('details');details.append(node('summary',t.deadline_missed?'⚠ Дедлайн превышен':'Почему эти даты'));for(const reason of t.reasons)details.append(node('p',reason));why.append(details);tr.append(why);if(t.deadline_missed)tr.classList.add('deadline-warning');$('output').append(tr);
 }
}
function toggle(id){if(dirty){status('Сначала примените изменения или отмените ввод.',true);return;}if(collapsed.has(id))collapsed.delete(id);else collapsed.add(id);renderEditor();renderSchedule();syncButtons();}
function adopt(next){state=next;dirty=false;renderEditor();renderSchedule();renderResources();renderManagement();syncButtons();}
function commandsFromFields(){
 if(!$('start').value)throw Error('Укажите дату начала');
 const rows=['project\t'+clean($('title').value)+'\t'+$('start').value+'\t'+clean($('holidays').value)];
 const week=new Map();for(const line of $('weekly').value.split('\n').map(s=>s.trim()).filter(Boolean)){const [day,...hours]=line.split('=');if(!/^[0-6]$/.test(day)||week.has(day)||hours.length!==1)throw Error('Укажите 7 строк календаря: 0=09:00-17:00, ..., 6=-');week.set(day,clean(hours[0]));}if(week.size!==7)throw Error('Нужны все 7 дней недели');
 const exceptions=$('exceptions').value.split('\n').map(s=>s.trim()).filter(Boolean).map(clean).join(';')||'-';
 rows.push('calendar\t'+$('start-time').value+'\t'+clean($('offset').value)+'\t'+Array.from({length:7},(_,i)=>week.get(String(i))).join(';')+'\t'+exceptions);

 rows.push(...resourceCommands());
 for(const tr of $('tasks').children){const id=Number(tr.dataset.id),t=taskById(id);const v={};for(const e of tr.querySelectorAll('[data-key]'))v[e.dataset.key]=e.value;
  if(v.title!==t.title||Number(v.duration)!==t.duration)rows.push('update\t'+id+'\t'+(t.kind==='summary'?'0':v.duration)+'\t'+clean(v.title));
  if(t.kind==='task'){
   rows.push('predecessors\t'+id+'\t'+clean(v.links).toUpperCase());
   if(v.constraint_type==='-'&&v.constraint_at)throw Error('Задача '+id+': выберите тип ограничения или очистите его дату');
   if(v.constraint_type!=='-'&&!v.constraint_at)throw Error('Задача '+id+': укажите дату и время ограничения');
   rows.push('dates\t'+id+'\t'+v.constraint_type+'\t'+(v.constraint_at||'-')+'\t'+(v.deadline||'-'));
   if(assignmentWire(v.assignments)!==state.schedule.resource_report.assignments.filter(a=>a.task===id).map(a=>a.resource+':'+a.units_bp).join(','))rows.push('assignments\t'+id+'\t'+assignmentWire(v.assignments));
  }
  if(v.parent!==String(t.parent??'-'))rows.push('move\t'+id+'\t'+v.parent+'\t-');
 }
 rows.push(...managementCommands());
 return rows;
}
async function send(rows,message){
 if(busy)return false;busy=true;syncButtons();status('Применение команд в Rust…');
 try{const response=await fpFetch('/api/commands',{method:'POST',headers:{'Content-Type':'text/plain; charset=utf-8'},body:'revision\t'+state.revision+'\n'+rows.join('\n')+'\n'});const next=await response.json();if(!response.ok)throw Error(next.error+(response.status===409?' — обновите состояние кнопкой «Загрузить с сервера». Локальный ввод сохранён.':''));adopt(next);status(message);return true;}
 catch(e){status(e.message,true);return false;}finally{busy=false;syncButtons();}
}
async function structural(extra){if(busy)return;try{await send([...(dirty?commandsFromFields():[]),...extra],'Изменения применены. Расписание рассчитано.');}catch(e){status(e.message,true);}}
function addTask(kind,parent=null){if(!state?.next_id){status('Диапазон идентификаторов исчерпан.',true);return;}structural(['add\t'+state.next_id+'\t'+(parent??'-')+'\t-\t'+kind+'\t'+(kind==='summary'?0:480)+'\t'+(kind==='summary'?'Новый этап':'Новая задача')]);}
async function apply(){try{return await send(commandsFromFields(),'Изменения применены. Расписание рассчитано.');}catch(e){status(e.message,true);return false;}}
$('apply').onclick=apply;$('add').onclick=()=>addTask('task');$('phase').onclick=()=>addTask('summary');
$('undo').onclick=()=>send(['undo'],'Команда отменена.');$('redo').onclick=()=>send(['redo'],'Команда повторена.');
$('discard').onclick=()=>{adopt(state);status('Неприменённый ввод отменён.');};
$('refresh').onclick=async()=>{if(dirty){status('Сначала примените изменения или нажмите «Отменить ввод». Загрузка заменит поля данными сервера.',true);return;}if(busy)return;busy=true;syncButtons();try{const r=await fpFetch('/api/state');if(!r.ok)throw Error('Сервер недоступен');adopt(await r.json());status('Состояние загружено с сервера.');}catch(e){status(e.message,true);}finally{busy=false;syncButtons();}};
$('save').onclick=async()=>{if(busy)return;if(dirty&&!await apply())return;const blob=new Blob([state.document],{type:'text/plain;charset=utf-8'}),url=URL.createObjectURL(blob),a=node('a');a.href=url;a.download='project.fplan';a.click();savedDocument=state.document;setTimeout(()=>URL.revokeObjectURL(url),1000);status('Файл JSON подготовлен к скачиванию.');};
$('open').onchange=async e=>{const f=e.target.files[0];if(!f)return;if(dirty){status('Перед открытием примените изменения или отмените ввод.',true);e.target.value='';return;}if(f.size>7900000){status('Файл слишком большой.',true);e.target.value='';return;}try{const text=await f.text();if(/\.(xml|csv)$/i.test(f.name)){if(!workspaceMode)throw Error('XML/CSV импорт доступен в режиме workspace');const kind=/\.xml$/i.test(f.name)?'xml':'csv';const r=await fpFetch('/api/import.'+kind,{method:'POST',headers:{'Content-Type':'text/plain'},body:text});const v=await r.json();if(!r.ok)throw Error(v.error);pendingImport={document:v.document,revision:state.revision};$('exchange-warnings').textContent=v.warnings.join('\n');$('apply-import').disabled=false;status('Импорт подготовлен. Проверьте предупреждения и подтвердите применение.');return;}const ok=await send(['replace\n'+text],'Проект открыт. Проект открыт; старые форматы преобразованы в JSON.');if(ok){savedDocument=state.document;collapsed.clear();renderEditor();renderSchedule();syncButtons();}}catch(err){status(err.message,true);}finally{e.target.value='';}};
for(const id of['title','start','holidays','start-time','offset','weekly','exceptions'])$(id).oninput=changed;
document.addEventListener('keydown',e=>{if(e.ctrlKey||e.metaKey){if(e.key.toLowerCase()==='s'){e.preventDefault();$('save').click();}if(e.key.toLowerCase()==='z'&&!['INPUT','TEXTAREA','SELECT'].includes(document.activeElement.tagName)){e.preventDefault();(e.shiftKey?$('redo'):$('undo')).click();}}});
window.addEventListener('beforeunload',e=>{if(dirty||(state&&savedDocument!==state.document)){e.preventDefault();e.returnValue='';}});


// Resource capacities and assignment units use exact integer basis points on the wire.
function percentToBp(value){const text=value.trim();if(!/^\d+(\.\d{1,2})?$/.test(text))throw Error('Процент должен быть неотрицательным числом, не более двух знаков после точки');const [whole,fraction='']=text.split('.');const bp=Number(whole)*100+Number(fraction.padEnd(2,'0'));if(!Number.isSafeInteger(bp)||bp>1000000)throw Error('Процент превышает 10000%');return bp;}
function assignmentWire(text){return text.split(',').map(s=>s.trim()).filter(Boolean).map(entry=>{const match=/^(\d+):(.+)$/.exec(entry);if(!match)throw Error('Назначение: ID ресурса:процент, например 1:100');return match[1]+':'+percentToBp(match[2]);}).join(',');}
function periodsWire(text){const periods=text.split(';').map(s=>s.trim()).filter(Boolean).map(entry=>{const match=/^(\d{4}-\d{2}-\d{2})\.\.(\d{4}-\d{2}-\d{2}):(.+)$/.exec(entry);if(!match)throw Error('Доступность: YYYY-MM-DD..YYYY-MM-DD:процент');return match[1]+'..'+match[2]+':'+percentToBp(match[3]);});return periods.join(';')||'-';}
function resourceCommands(){const commands=[];for(const tr of $('resources').children){const id=tr.dataset.id,v={};for(const e of tr.querySelectorAll('[data-resource-key]'))v[e.dataset.resourceKey]=e.value;commands.push('resource-update\t'+id+'\t'+v.kind+'\t'+percentToBp(v.capacity)+'\t'+clean(v.name)+'\t'+periodsWire(v.periods));commands.push(typed('rate',{id:Number(id),cents_per_hour:scaled(v.rate,2)}));const previous=state.schedule.management.settings.resource_calendars[id];const profile=v.personal.trim()?{weekly:parseWeekProfile(v.personal),exceptions:previous?.exceptions||{}}:null;if(JSON.stringify(profile)!==JSON.stringify(previous||null))commands.push(typed('personal_calendar',{id:Number(id),calendar:profile}));}return commands;}
function renderResources(){
 const report=state.schedule.resource_report;const selected=$('load-resource').value;$('resources').replaceChildren();$('resource-summary').replaceChildren();$('load-resource').replaceChildren();
 for(const resource of report.resources){const tr=node('tr');tr.dataset.id=resource.id;tr.append(node('td',String(resource.id)));
  for(const [key,value]of [['name',resource.name],['kind',resource.kind],['capacity',String(resource.capacity_bp/100)],['periods',resource.availability.map(a=>a.from+'..'+a.through+':'+a.capacity_bp/100).join('; ')],['rate',String((state.schedule.management.settings.hourly_rates_cents[resource.id]||0)/100)],['personal',personalText(resource.id)]] ){
   const td=node('td');let e;if(key==='kind'){e=node('select');for(const [kind,title]of[['person','Человек'],['generic','Роль / команда']]){const option=node('option',title);option.value=kind;e.append(option);}e.value=value;e.onchange=changed;}else{e=input('resource-'+key,value,key==='capacity'?'number':'text',resource.id);if(key==='capacity'){e.max='10000';e.step='0.01';}e.setAttribute('aria-label',key+' ресурса '+resource.id);}e.dataset.resourceKey=key;td.append(e);tr.append(td);
  }const td=node('td');td.append(button('×','Удалить ресурс '+resource.id,()=>structural(['resource-delete\t'+resource.id])));tr.append(td);$('resources').append(tr);
  const summary=node('tr',undefined,resource.overloaded_minutes>0?'resource-overloaded':'');
  for(const value of[resource.id+' · '+resource.name,(resource.planned_minutes/60).toFixed(2),(resource.capacity_minutes/60).toFixed(2),(resource.excess_minutes/60).toFixed(2),resource.capacity_minutes>0?(100*resource.planned_minutes/resource.capacity_minutes).toFixed(1)+'%':'Нет доступности',resource.peak_units_bp/100+'%',resource.overloaded_minutes>0?'⚠ '+resource.overloaded_minutes+' мин.':'Без перегрузки'])summary.append(node('td',String(value)));$('resource-summary').append(summary);
  const option=node('option',resource.id+' · '+resource.name);option.value=resource.id;$('load-resource').append(option);
 }
 if(report.resources.length){const valid=report.resources.some(r=>String(r.id)===selected);$('load-resource').value=valid?selected:String((report.resources.find(r=>r.overloaded_minutes>0)||report.resources[0]).id);}
 $('load-resource').disabled=report.resources.length===0;$('resource-empty').hidden=report.resources.length>0;renderLoad();
}
function renderLoad(){
 const report=state?.schedule.resource_report;if(!report)return;const resource=report.resources.find(r=>String(r.id)===$('load-resource').value);$('resource-daily').replaceChildren();$('resource-overloads').replaceChildren();$('resource-report-note').textContent='';if(!resource)return;
 for(const row of resource.daily){const tr=node('tr',undefined,row.overloaded_minutes>0?'resource-overloaded':'');for(const value of[row.date,(row.planned_minutes/60).toFixed(2),(row.capacity_minutes/60).toFixed(2),(row.excess_minutes/60).toFixed(2),row.peak_units_bp/100+'%',row.overloaded_minutes])tr.append(node('td',String(value)));$('resource-daily').append(tr);}
 for(const period of resource.overloads){const tr=node('tr');const names=period.task_ids.map(id=>{const t=taskById(id);return '#'+id+' '+(t?.title??'');}).join(', ')+(period.active_task_count>period.task_ids.length?' … и ещё '+(period.active_task_count-period.task_ids.length):'');for(const value of[displayTime(period.start),displayTime(period.finish),period.working_minutes,period.demand_bp/100+'%',period.capacity_bp/100+'%',names])tr.append(node('td',String(value)));$('resource-overloads').append(tr);}
 $('resource-report-note').textContent='Итоги рассчитаны для всего плана. '+(resource.daily_truncated?'Дневная таблица ограничена первыми '+report.daily_limit+' рабочими днями. ':'Дневная таблица показана полностью. ')+(resource.overloads_truncated?'Показаны первые '+report.overload_limit+' интервалов перегрузки из '+resource.overload_count+'.':'Интервалов перегрузки: '+resource.overload_count+'.');
}
$('resource-add').onclick=()=>{if(!state?.next_resource_id){status('Диапазон ID ресурсов исчерпан.',true);return;}structural(['resource-add\t'+state.next_resource_id+'\tperson\t10000\tНовый ресурс\t-']);};
$('load-resource').onchange=renderLoad;

function typed(type,data){return 'json\t'+JSON.stringify({type,data});}
function scaled(text,places){if(!new RegExp('^\\d+(\\.\\d{1,'+places+'})?$').test(text))throw Error('Число должно быть неотрицательным с точностью до '+places+' знаков');const [whole,fraction='']=text.split('.');const n=Number(whole)*10**places+Number(fraction.padEnd(places,'0'));if(!Number.isSafeInteger(n))throw Error('Значение слишком велико');return n;}
function personalText(id){const cal=state.schedule.management.settings.resource_calendars[id];return cal?cal.weekly.map(day=>day.length?day.map(i=>time(i.start)+'-'+time(i.end)).join(','):'-').join(';'):'';}
function time(m){return String(Math.floor(m/60)).padStart(2,'0')+':'+String(m%60).padStart(2,'0');}
function parseWeekProfile(text){const days=text.split(';');if(days.length!==7)throw Error('Личный календарь: 7 дней через ;');return days.map(day=>day.trim()==='-'?[]:day.split(',').map(part=>{const m=/^(\d\d):(\d\d)-(\d\d):(\d\d)$/.exec(part.trim());if(!m)throw Error('Интервал HH:MM-HH:MM');return{start:Number(m[1])*60+Number(m[2]),end:Number(m[3])*60+Number(m[4])};}));}
function renderManagement(){
 $('management').replaceChildren();for(const t of state.schedule.management.tasks){const task=taskById(t.id),tr=node('tr');tr.dataset.id=t.id;tr.append(node('td',t.id+' · '+task.title));
 const mode=node('select');for(const value of['fixed_duration','fixed_work','fixed_units']){const e=node('option',value);e.value=value;mode.append(e);}mode.value=t.policy.mode;mode.dataset.m='mode';mode.onchange=changed;let td=node('td');td.append(mode);tr.append(td);
 const effort=input('effort','', 'checkbox',t.id);effort.checked=t.policy.effort_driven;effort.dataset.m='effort';td=node('td');td.append(effort);tr.append(td);
 for(const [key,value]of[['work',String(t.work_basis/10000)],['fixed',String((state.schedule.management.settings.fixed_costs_cents[t.id]||0)/100)],['percent',String(t.actual.percent_complete)],['actual_work',String(t.actual.work_basis/10000)],['actual_cost',String(t.actual.cost_cents/100)]]){const e=input(key,value,'text',t.id);e.dataset.m=key;td=node('td');td.append(e);tr.append(td);}tr.append(node('td',(t.planned_cost_cents/100).toFixed(2)));$('management').append(tr);
 }
 $('status-date').value=state.schedule.management.settings.status_date||'';$('baseline-list').textContent=state.schedule.management.settings.baselines.map(b=>b.name).join(', ')||'Базовых планов нет';$('evm').textContent=JSON.stringify(state.schedule.management.evm,null,2);
}
function managementCommands(){const rows=[];for(const tr of $('management').children){const id=Number(tr.dataset.id),old=state.schedule.management.tasks.find(t=>t.id===id),v={};for(const e of tr.querySelectorAll('[data-m]'))v[e.dataset.m]=e.type==='checkbox'?e.checked:e.value;
 if(v.mode!==old.policy.mode||v.effort!==old.policy.effort_driven)rows.push(typed('policy',{id,policy:{mode:v.mode,effort_driven:v.effort,target_work_basis:v.mode==='fixed_work'?old.work_basis:null}}));
 const target=scaled(v.work,4);if(target!==old.work_basis)rows.push(typed('work',{id,target_basis:target}));const fixed=scaled(v.fixed,2);if(fixed!==(state.schedule.management.settings.fixed_costs_cents[id]||0))rows.push(typed('fixed_cost',{id,cents:fixed}));
 const actual={...old.actual,percent_complete:Number(v.percent),work_basis:scaled(v.actual_work,4),cost_cents:scaled(v.actual_cost,2)};if(JSON.stringify(actual)!==JSON.stringify(old.actual))rows.push(typed('actual',{id,actual}));
 }const date=$('status-date').value||null;if(date!==state.schedule.management.settings.status_date)rows.push(typed('status_date',date));return rows;}
$('capture-baseline').onclick=()=>{const name=$('baseline-name').value.trim();if(!name){status('Укажите имя базового плана',true);return;}structural([typed('baseline',{name})]);};
let workspaceMode=false,previewBase=null,pendingImport=null;
async function workspaceInit(){try{const response=await fpFetch('/api/capabilities');if(!response.ok){if(response.status===401){$('workspace-tools').hidden=false;$('workspace-info').textContent='Требуется токен API';}return;}const cap=await response.json();workspaceMode=cap.persistent;userRole=cap.role;syncButtons();$('publish').disabled=!['manager','admin'].includes(userRole);$('new-project').disabled=userRole!=='admin';$('workspace-tools').hidden=false;$('workspace-info').textContent='Постоянное хранилище · '+cap.organization+' · '+cap.role;await loadProjects();}catch(e){status(e.message,true);}}
async function loadProjects(){const response=await fpFetch('/api/projects');const result=await response.json();if(!response.ok)throw Error(result.error);$('projects').replaceChildren();for(const p of result.projects){const e=node('option',p.title+' ['+p.id+']');e.value=p.id;$('projects').append(e);}$('projects').value=activeProject;const portfolio=$('portfolio');portfolio.replaceChildren();for(const p of result.projects){const tr=node('tr');for(const value of[p.id,p.title,p.revision,p.tasks,p.working_minutes,(p.planned_cost_cents/100).toFixed(2),p.published?'Да':'Нет'])tr.append(node('td',String(value)));portfolio.append(tr);}}
$('projects').onchange=async()=>{if(dirty){status('Сначала примените или отмените ввод',true);$('projects').value=activeProject;return;}activeProject=$('projects').value;const r=await fpFetch('/api/state');if(r.ok)adopt(await r.json());};
$('connect-token').onclick=async()=>{sessionStorage.setItem('fp-token',$('api-token').value);sessionStorage.setItem('fp-org',$('organization').value||'local');try{await loadProjects();const r=await fpFetch('/api/state');if(!r.ok)throw Error((await r.json()).error);adopt(await r.json());status('Подключено');}catch(e){status(e.message,true);}};
$('new-project').onclick=async()=>{const id=prompt('ID проекта: латинские буквы, цифры, дефис');if(!id)return;try{const r=await fpFetch('/api/projects',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id,title:id})});const result=await r.json();if(!r.ok)throw Error(result.error);await loadProjects();status('Проект создан');}catch(e){status(e.message,true);}};
$('publish').onclick=async()=>{try{const r=await fpFetch('/api/publish',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({base_revision:state.revision})});const v=await r.json();if(!r.ok)throw Error(v.error);status('Текущая ревизия опубликована');await loadProjects();}catch(e){status(e.message,true);}};
$('preview-leveling').onclick=async()=>{if(dirty){status('Сначала примените изменения',true);return;}try{const r=await fpFetch('/api/leveling/preview',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({within_float:$('within-float').checked})});const v=await r.json();if(!r.ok)throw Error(v.error);previewBase=v.base_revision;$('leveling-result').textContent=JSON.stringify({shifts:v.preview.shifts,unresolved:v.preview.unresolved,warnings:v.preview.warnings},null,2);$('apply-leveling').disabled=v.preview.unresolved>0;}catch(e){status(e.message,true);}};
$('apply-leveling').onclick=async()=>{try{const r=await fpFetch('/api/leveling/apply',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({base_revision:previewBase,within_float:$('within-float').checked})});const v=await r.json();if(!r.ok)throw Error(v.error);adopt(v);status('Выравнивание применено. Можно отменить.');}catch(e){status(e.message,true);}};
function download(text,name,type){const url=URL.createObjectURL(new Blob([text],{type})),a=node('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
$('export-csv').onclick=async()=>{const r=await fpFetch('/api/export.csv');if(r.ok)download(await r.text(),'project.csv','text/csv');};
$('export-xml').onclick=async()=>{const r=await fpFetch('/api/export.xml');const v=await r.json();if(r.ok){$('exchange-warnings').textContent=v.warnings.join('\n');download(v.xml,'project.xml','application/xml');}else status(v.error,true);};
$('apply-import').onclick=async()=>{if(!pendingImport)return;if(pendingImport.revision!==state.revision){status('Импорт устарел: откройте файл повторно',true);return;}if(await send(['replace\n'+pendingImport.document],'Импорт применён')){pendingImport=null;$('apply-import').disabled=true;}};
$('refresh-portfolio').onclick=()=>loadProjects().catch(e=>status(e.message,true));
workspaceInit();
let oidcConfiguration=null;
function b64url(bytes){return btoa(String.fromCharCode(...bytes)).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');}
$('oidc-login').onclick=async()=>{if(!oidcConfiguration)return;const verifier=b64url(crypto.getRandomValues(new Uint8Array(32))),loginState=crypto.randomUUID();const challenge=b64url(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(verifier))));sessionStorage.setItem('fp-pkce',JSON.stringify({verifier,state:loginState}));const target=new URL(oidcConfiguration.authorization_endpoint);for(const [key,value]of Object.entries({client_id:oidcConfiguration.client_id,response_type:'code',scope:'openid profile email',redirect_uri:location.origin+'/',state:loginState,code_challenge:challenge,code_challenge_method:'S256'}))target.searchParams.set(key,value);location.assign(target);};
$('oidc-logout').onclick=()=>{sessionStorage.removeItem('fp-token');sessionStorage.removeItem('fp-pkce');location.assign(location.origin+'/');};
async function initializeOidc(){try{const r=await nativeFetch('/api/auth/config');if(!r.ok)return;const config=await r.json();if(!config)return;oidcConfiguration=config;$('oidc-login').hidden=false;$('oidc-logout').hidden=false;$('organization').value=config.organization;sessionStorage.setItem('fp-org',config.organization);
 const params=new URLSearchParams(location.search);if(params.has('error'))throw Error('OIDC: '+params.get('error'));if(!params.has('code'))return;const pending=JSON.parse(sessionStorage.getItem('fp-pkce')||'null');sessionStorage.removeItem('fp-pkce');history.replaceState(null,'',location.pathname);if(!pending||params.get('state')!==pending.state)throw Error('OIDC state mismatch');
 const response=await nativeFetch(config.token_endpoint,{method:'POST',headers:{'Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams({grant_type:'authorization_code',client_id:config.client_id,code:params.get('code'),redirect_uri:location.origin+'/',code_verifier:pending.verifier})});if(!response.ok)throw Error('OIDC token exchange failed');const tokens=await response.json();if(!tokens.access_token)throw Error('OIDC access token missing');sessionStorage.setItem('fp-token',tokens.access_token);await workspaceInit();const stateResponse=await fpFetch('/api/state');if(!stateResponse.ok)throw Error((await stateResponse.json()).error);adopt(await stateResponse.json());savedDocument=state.document;status('Вход через OIDC выполнен');
 }catch(e){status(e.message,true);}}
initializeOidc();
$('submit-time').onclick=async()=>{try{const r=await fpFetch('/api/timesheets',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({task:Number($('time-task').value),resource:Number($('time-resource').value),date:$('time-date').value,minutes:Number($('time-minutes').value)})});const v=await r.json();if(!r.ok)throw Error(v.error);await loadTimesheets();status('Табель отправлен на согласование');}catch(e){status(e.message,true);}};
async function loadTimesheets(){const r=await fpFetch('/api/timesheets');const v=await r.json();if(!r.ok)throw Error(v.error);$('timesheets').replaceChildren();for(const t of v.timesheets){const tr=node('tr');for(const value of[t.id,t.task,t.resource,t.date,t.minutes,t.actor,t.status])tr.append(node('td',String(value)));const td=node('td');if(t.status==='submitted')td.append(button('Утвердить','Утвердить табель '+t.id,async()=>{if(dirty){status('Сначала примените изменения',true);return;}const r=await fpFetch('/api/timesheets/approve',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({id:t.id})});const result=await r.json();if(!r.ok){status(result.error,true);return;}adopt(await(await fpFetch('/api/state')).json());await loadTimesheets();}));tr.append(td);$('timesheets').append(tr);}}
$('refresh-timesheets').onclick=()=>loadTimesheets().catch(e=>status(e.message,true));
(async()=>{busy=true;syncButtons();try{const response=await fpFetch('/api/state');if(!response.ok)throw Error('Ошибка загрузки');const next=await response.json();savedDocument=next.document;adopt(next);status('Проект загружен. Расписание рассчитано.');}catch(e){status(e.message,true);}finally{busy=false;syncButtons();}})();
