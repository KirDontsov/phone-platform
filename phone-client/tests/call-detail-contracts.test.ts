import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import CallDetailPage from '../src/pages/call-detail/CallDetailPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me,CallDetail,Taxonomy,Analysis} from '../src/shared/interfaces'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
async function render(){
 const pinia=createPinia();setActivePinia(pinia);useAuth().adopt({user:{id:'u',email:'owner@example.test',name:'Owner'},organization:{id:'o',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'o',name:'Company',role:'owner'}],role:'owner',csrf_token:'csrf',demo_mode:true} as Me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/calls/:id',component:{template:'<div/>'}},{path:'/calls',component:{template:'<div/>'}}]});await router.push('/calls/c1');await router.isReady()
 const analysis:Analysis={id:'a',state:'ready',model:'demo',provider:'demo',prompt_version:'1',rubric_version:3,transcript_id:'t',summary:'Summary',customer_intent:'',agreements:[],next_steps:[],suggested_status_id:null,suggested_tag_ids:[],criteria:[],score:null,abstention_reason:'Нет достаточных доказательств',created_at:'2026-11-11T12:00:00Z',usage:{input_tokens:10,output_tokens:5,cost:null}}
 const data:CallDetail={call:{id:'c1',provider_id:'p1',started_at:'2026-11-11T12:00:00Z',phone:'synthetic',contact_name:'Client',employee:'Manager',direction:'in',provider_result:'bridged',human_connected:true,duration:30,stage:'ready',status_id:'old-status',tag_ids:['old-tag'],contact_kind:'unknown',score:null,has_recording:false,note:'Retain me',manual_lock:true,version:7,coverage_complete:false},legs:[],recordings:[],analyses:[analysis],history:[],audit:[],external_links:[],transcripts:[]}
 const taxonomy:Taxonomy={statuses:[{id:'old-status',name:'Старый статус',color:'#000000',archived:true},{id:'unused-status',name:'Чужой архивный статус',color:'#000000',archived:true},{id:'new-status',name:'Новый статус',color:'#000000',archived:false}],tags:[{id:'old-tag',name:'Старый тег',color:'#000000',archived:true},{id:'unused-tag',name:'Чужой архивный тег',color:'#000000',archived:true},{id:'new-tag',name:'Новый тег',color:'#000000',archived:false}]}
 const writes:{method:string;url:string;body:unknown}[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='PATCH'){const body=JSON.parse(String(options.body));writes.push({method:'PATCH',url,body});Object.assign(data.call,body,{version:8})}return new Response(JSON.stringify({data:url==='/api/taxonomy'?taxonomy:options.method==='PATCH'?data.call:data}),{headers:{'Content-Type':'application/json'}})}))
 const wrapper=mount(CallDetailPage,{global:{plugins:[pinia,router]}});await flushPromises();return{wrapper,writes}
}
describe('call detail nullable values and archived taxonomy contracts',()=>{
 it('retains historical archived labels while excluding unassigned archive entries and preserves optimistic write fields',async()=>{
  const{wrapper,writes}=await render();try{expect(wrapper.text()).toContain('Старый статус (архив)');expect(wrapper.text()).toContain('Старый тег (архив)');expect(wrapper.text()).not.toContain('Чужой архивный');const note=wrapper.find('textarea');await note.setValue('Changed');await wrapper.findAll('button').find(b=>b.text()==='Сохранить изменения')!.trigger('click');await flushPromises();expect(writes).toEqual([{method:'PATCH',url:'/api/calls/c1',body:{status_id:'old-status',tag_ids:['old-tag'],note:'Changed',contact_kind:'unknown',manual_lock:true,version:7}}]);await wrapper.findAll('button').find(b=>b.text()==='Старый тег (архив)')!.trigger('click');expect(wrapper.text()).not.toContain('Старый тег (архив)')}finally{wrapper.unmount()}
 })
 it('shows missing analysis pricing and abstention without manufacturing a zero price or score',async()=>{
  const{wrapper}=await render();try{await wrapper.findAll('button').find(b=>b.text().includes('Анализ диалога'))!.trigger('click');expect(wrapper.text()).toContain('стоимость неизвестна');expect(wrapper.text()).toContain('Нет достаточных доказательств');expect(wrapper.find('.score-large').text()).toContain('—');expect(wrapper.text()).toContain('версия критериев 3')}finally{wrapper.unmount()}
 })
})
