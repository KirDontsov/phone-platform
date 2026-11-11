import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import CallDetailPage from '../src/pages/call-detail/CallDetailPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me,CallDetail} from '../src/shared/interfaces'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
describe('manual transcript role confirmation',()=>{
 it('assigns only existing speakers and selects the new immutable transcript version',async()=>{
 const pinia=createPinia();setActivePinia(pinia);const auth=useAuth();auth.adopt({user:{id:'u',email:'user@example.test',name:'Owner'},organization:{id:'org',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'org',name:'Company',role:'owner'}],role:'owner',csrf_token:'csrf',demo_mode:true} as Me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/calls/:id',component:{template:'<div/>'}},{path:'/calls',component:{template:'<div/>'}}]});await router.push('/calls/c1');await router.isReady()
 const data:CallDetail={call:{id:'c1',provider_id:'p1',started_at:'2026-11-11T12:00:00Z',phone:'synthetic',contact_name:'Client',employee:'Manager',direction:'in',provider_result:'bridged',human_connected:true,duration:30,stage:'partial',status_id:null,tag_ids:[],contact_kind:'unknown',score:null,has_recording:false,note:'',manual_lock:false,version:1,coverage_complete:false},legs:[],recordings:[],analyses:[],history:[],audit:[],external_links:[],transcripts:[{id:'tr1',version:1,provider:'http',state:'ready',language:'ru',role_confidence:'uncertain',created_at:'2026-11-11T12:00:00Z',segments:[{id:'s1',speaker:'A',role:'unknown',start:0,end:10,text:'Hello'},{id:'s2',speaker:'B',role:'unknown',start:10,end:20,text:'Question'},{id:'s3',speaker:'A',role:'unknown',start:20,end:30,text:'Answer'}]}]}
 const sent:{url:string;body:unknown}[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{let result:unknown=data;if(url==='/api/taxonomy')result={tags:[],statuses:[]};else if(options.method==='POST'){const body=JSON.parse(String(options.body));sent.push({url,body});const source=data.transcripts[0]!;const updated={...source,id:'tr2',version:2,role_confidence:'known',segments:source.segments.map(s=>({...s,role:body.roles[s.speaker]}))};data.transcripts=[updated,source];result={transcript:updated,job_id:'job1'}}return new Response(JSON.stringify({data:result}),{headers:{'Content-Type':'application/json'}})}))
 const wrapper=mount(CallDetailPage,{global:{plugins:[pinia,router]}});try{await flushPromises();expect(wrapper.text()).toContain('Роли участников требуют проверки');await wrapper.findAll('button').find(b=>b.text()==='Назначить роли')!.trigger('click');const selects=wrapper.findAll('.role-assignment select');expect(selects).toHaveLength(2);await selects[0]!.setValue('manager');await selects[1]!.setValue('client');await wrapper.findAll('form').find(f=>f.text().includes('Сохранить роли и анализировать'))!.trigger('submit');await flushPromises();expect(sent).toEqual([{url:'/api/calls/c1/transcripts/tr1/roles',body:{roles:{A:'manager',B:'client'}}}]);expect(wrapper.text()).toContain('Роли участников определены');expect(data.transcripts[1]!.segments[0]!.role).toBe('unknown')}finally{wrapper.unmount()}
 })
})
