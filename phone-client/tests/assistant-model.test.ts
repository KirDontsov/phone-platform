import {afterEach,describe,it,expect,vi} from 'vitest'
import {useAssistant} from '../src/entities/assistant/model'
import {resetContext} from '../src/shared/api/client'
import {validatedRun,validatedMessages} from '../src/shared/api/assistant'
import {assistantPinia,assistantMe,conversation,run,userMessage,assistantMessage,disconnected,credential,response} from './assistant-fixtures'
let cleanup:(()=>void)|undefined
afterEach(()=>{cleanup?.();cleanup=undefined;resetContext();vi.useRealTimers();vi.unstubAllGlobals()})
function setup(){const context=assistantPinia(),store=useAssistant();cleanup=()=>store.reset();return{...context,store}}
describe('assistant durable state and identity',()=>{
 it('keeps missing-auth text out of history and retains the task for an authenticated retry',async()=>{
  const{store}=setup();const sent:{url:string;body:any}[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='POST')sent.push({url,body:JSON.parse(String(options.body))});if(url.endsWith('/conversations'))return response(conversation);return response({code:'assistant_auth_required',message:'Подключите Codex'},409)}));store.draft='Создай теги';expect(await store.send()).toBe(false);expect(store.messages).toEqual([]);expect(store.draft).toBe('Создай теги');expect(store.authRequired).toBe(true);expect(sent[1]!.body).toMatchObject({text:'Создай теги'});expect(sent[1]!.body.client_request_id).toMatch(/^[0-9a-f-]{36}$/);store.adoptStatus({...disconnected,authenticated:true,auth_mode:'apiKey'});expect(store.authRequired).toBe(false);expect(store.error).toBe('')
 })
 it('reuses the identical request id after a lost message response and creates no optimistic ghost messages',async()=>{
  vi.useFakeTimers();const{store}=setup();const requests:any[]=[];let attempts=0;vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(url.endsWith('/messages')){requests.push(JSON.parse(String(options.body)));if(++attempts===1)throw Error('lost response');return response({run,user_message:userMessage,assistant_message:assistantMessage},202)}if(options.method==='POST')return response(conversation);return response({items:[conversation],next_cursor:null})}));store.draft='Создай теги';await store.send();expect(store.messages).toEqual([]);expect(store.uncertainSubmit).toBe(true);await store.send();expect(requests).toHaveLength(2);expect(requests[1]).toEqual(requests[0]);expect(store.messages).toHaveLength(2);expect(store.uncertainSubmit).toBe(false)
 })
 it('polls partial text and durable tool events, then stops only on a terminal run state',async()=>{
  vi.useFakeTimers();const{store}=setup();let polls=0;vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(url.endsWith('/messages'))return response({run,user_message:userMessage,assistant_message:assistantMessage},202);if(url.includes('/runs/')){polls++;return response({run:{...run,state:polls===1?'running':'completed',events:[{seq:2,type:'tool',tool_name:'upsert_taxonomy',title:'Создан тег',status:polls===1?'running':'completed'}]},messages:[{...assistantMessage,text:polls===1?'Проверяю':'Готово',status:polls===1?'running':'completed'}]})}if(options.method==='POST')return response(conversation);return response({items:[conversation],next_cursor:null})}));store.draft='Создай теги';await store.send();await vi.advanceTimersByTimeAsync(1000);expect(store.messages).toHaveLength(2);expect(store.messages[1]!.text).toBe('Проверяю');expect(store.busy).toBe(true);await vi.advanceTimersByTimeAsync(1000);expect(store.messages[1]!.text).toBe('Готово');expect(store.currentRun?.events[0]!.status).toBe('completed');expect(store.busy).toBe(false);await vi.advanceTimersByTimeAsync(10000);expect(polls).toBe(2)
 })
 it('cancels on the server while preserving already committed tool receipts',async()=>{
  const{store}=setup();store.conversation={...conversation,active_run_id:run.id};store.runs=[{...run,events:[{seq:1,type:'tool',title:'Тег создан',status:'completed'}]}];const calls:string[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string)=>{calls.push(url);return response({run:{...store.currentRun,state:'cancelled'}})}));await store.stop();expect(calls).toEqual(['/api/assistant/runs/run-1/cancel']);expect(store.currentRun?.state).toBe('cancelled');expect(store.currentRun?.events[0]!.status).toBe('completed');expect(store.notice).toContain('Уже выполненные изменения сохранены')
 })
 it('drops late history after a tenant switch even if the transport ignores AbortSignal',async()=>{
  const{store,auth}=setup();let release!:(value:Response)=>void;vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(resolve=>{release=resolve})));const pending=store.loadList();auth.adopt({...structuredClone(assistantMe),organization:{...assistantMe.organization,id:'org-2',name:'Компания Б'}});release(response({items:[conversation],next_cursor:null}));await pending;expect(store.conversations).toEqual([]);expect(store.conversation).toBe(null);expect(store.draft).toBe('')
 })
 it('never sends recognized credentials through the message endpoint',async()=>{
  const{store}=setup();const fetch=vi.fn();vi.stubGlobal('fetch',fetch);store.draft='{"app_secret":"PRIVACY_SENTINEL"}';await store.send();expect(fetch).not.toHaveBeenCalled();expect(store.messages).toEqual([]);expect(store.draft).toBe('');expect(store.error).not.toContain('PRIVACY_SENTINEL')
 })
 it('projects validated tool events and fails closed for an unknown state',()=>{
  const projected=validatedRun({...run,events:[{seq:3,type:'tool',title:'Тег создан',status:'completed',...{arguments:{app_secret:'PRIVACY_SENTINEL'}}}]});expect(JSON.stringify(projected.events)).not.toContain('PRIVACY_SENTINEL');expect(()=>validatedRun({...run,state:'made_up' as typeof run.state})).toThrow('Некорректное состояние')
 })
 it('bounds a stalled submit and preserves its uncertain result for idempotent retry',async()=>{
  vi.useFakeTimers();const{store}=setup();vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(url.endsWith('/messages'))return new Promise<Response>((_,reject)=>options.signal?.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true}));return response(conversation)}));store.draft='Создай теги';const pending=store.send();await vi.advanceTimersByTimeAsync(30001);expect(await pending).toBe(false);expect(store.uncertainSubmit).toBe(true);expect(store.submitting).toBe(false);expect(store.draft).toBe('Создай теги');expect(store.messages).toEqual([])
 })

 it('does not silently drop an expired credential selection from a new request',async()=>{
  const{store}=setup();store.credentials=[{...credential,expires_at:'2020-01-01T00:00:00Z'}];store.selectedRefs=[credential.id];store.draft='Подключи Телфин';const fetch=vi.fn();vi.stubGlobal('fetch',fetch);expect(await store.send()).toBe(false);expect(fetch).not.toHaveBeenCalled();expect(store.error).toContain('Срок выбранного доступа истёк')
 })

 it('restores durable chronological order by sequence and falls back to user before assistant for a legacy same-run tie',()=>{
  expect(validatedMessages([{...assistantMessage,sequence:2},{...userMessage,sequence:1}]).map(m=>m.id)).toEqual([userMessage.id,assistantMessage.id]);expect(validatedMessages([assistantMessage,userMessage]).map(m=>m.role)).toEqual(['user','assistant']);expect(validatedMessages([{...assistantMessage,sequence:-1}])[0]!.sequence).toBeUndefined()
 })

 it('stops polling and clears private content when an active conversation has been removed',async()=>{
  vi.useFakeTimers();const{store}=setup();let reads=0;vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(url.endsWith('/messages'))return response({run,user_message:userMessage,assistant_message:assistantMessage},202);if(url.includes('/runs/')){reads++;return response({code:'not_found',message:'Диалог недоступен'},404)}if(options.method==='POST')return response(conversation);return response({items:[conversation],next_cursor:null})}));store.draft='Создай теги';await store.send();await vi.advanceTimersByTimeAsync(1000);expect(store.messages).toEqual([]);expect(store.conversation).toBe(null);expect(store.error).toBe('Диалог недоступен');await vi.advanceTimersByTimeAsync(10000);expect(reads).toBe(1)
 })

})
