import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import SettingsPage from '../src/pages/settings/SettingsPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'
const localCatalog={available:true,providers:[{id:'apinex',name:'APInex',models:[{id:'apinex/free/test-model',name:'Existing free model'}]}],asr_models:[{id:'apinex/voice-stt',name:'APInex Voice STT',provider:'apinex'}]}
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
async function render(tab='analysis',initial:Record<string,unknown>={},catalog:unknown=localCatalog,role='owner'){
 const pinia=createPinia();setActivePinia(pinia);useAuth().adopt({user:{id:'u',email:'owner@example.test',name:'Owner'},organization:{id:'o',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'o',name:'Company',role}],role,csrf_token:'csrf',demo_mode:true} as Me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/settings/:tab',component:{template:'<div/>'}}]});await router.push('/settings/'+tab);await router.isReady()
 let config={...initial};const writes:{url:string;body:any}[]=[]
 vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{
  if(url.endsWith('/local-models')){if(catalog instanceof Error)throw catalog;return new Response(JSON.stringify({data:catalog}),{headers:{'Content-Type':'application/json'}})}
  if(options.method!=='GET'){const body=JSON.parse(String(options.body));writes.push({url,body});if(options.method==='PUT')config={...config,...body.config}}
  return new Response(JSON.stringify({data:{items:[{id:'conn',kind:tab,state:'configured',config,last_error:null,last_sync_at:null}]}}),{headers:{'Content-Type':'application/json'}})
 }))
 const wrapper=mount(SettingsPage,{global:{plugins:[pinia,router]}});await flushPromises();return{wrapper,writes}
}
function field(wrapper:ReturnType<typeof mount>,name:string){return wrapper.findAll('label').find(l=>l.text().startsWith(name))!}
describe('existing local OpenCode providers',()=>{
 it('selects an existing analysis model without requesting or sending a new API key',async()=>{
  const {wrapper,writes}=await render('analysis',{provider:'openai',model:'old',api_key:'<configured>',base_url:'https://api.openai.com/v1'});try{
   expect(writes).toHaveLength(0);await field(wrapper,'Провайдер').find('select').setValue('opencode')
   expect(wrapper.findAll('label').some(l=>l.text().startsWith('API-ключ'))).toBe(false)
   expect((field(wrapper,'Адрес API / сервера').find('input').element as HTMLInputElement).value).toBe('http://model-runner:4096');expect(field(wrapper,'Адрес API / сервера').find('input').attributes('readonly')).toBeDefined()
   const model=field(wrapper,'Модель').find('select');expect(model.findAll('option').some(o=>o.attributes('value')==='apinex/free/test-model')).toBe(true)
   expect(writes).toHaveLength(0);await model.setValue('apinex/free/test-model');await wrapper.find('form').trigger('submit');await flushPromises()
   expect(writes).toHaveLength(1);expect(writes[0]!.body.config).toMatchObject({provider:'opencode',model:'apinex/free/test-model',base_url:'http://model-runner:4096'});expect(writes[0]!.body.config).not.toHaveProperty('api_key');expect(wrapper.text()).toContain('уже подключённого провайдера')
  }finally{wrapper.unmount()}
 })
 it('requires an explicit local ASR model and saves it without an API key or automatic processing',async()=>{
  const {wrapper,writes}=await render('asr',{provider:'openai',model:'old',api_key:'<configured>',base_url:'https://api.openai.com/v1'});try{
   await field(wrapper,'Провайдер').find('select').setValue('opencode');expect((field(wrapper,'Доступ к моделям').find('select').element as HTMLSelectElement).value).toBe('local')
   expect(wrapper.findAll('label').some(l=>l.text().startsWith('API-ключ'))).toBe(false)
   const model=field(wrapper,'Модель').find('select');expect((model.element as HTMLSelectElement).value).toBe('');expect(model.text()).toContain('APInex Voice STT');expect(writes).toHaveLength(0)
   await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toHaveLength(0)
   await model.setValue('apinex/voice-stt');await wrapper.find('form').trigger('submit');await flushPromises()
   expect(writes).toHaveLength(1);expect(writes[0]!.body.config).toMatchObject({provider:'opencode',credential_source:'local',local_provider:'apinex',model:'apinex/voice-stt'});expect(writes[0]!.body.config).not.toHaveProperty('api_key');expect(writes[0]!.url).toBe('/api/connections/asr');expect(wrapper.text()).toContain('баланс')
  }finally{wrapper.unmount()}
 })
 it('keeps explicit OpenRouter access and the original two ASR models separate',async()=>{
  const {wrapper,writes}=await render('asr',{provider:'opencode',model:'moondream/parakeet-ultra',base_url:'http://model-runner:4096',api_key:'<configured>'});try{
   const access=field(wrapper,'Доступ к моделям').find('select');expect((access.element as HTMLSelectElement).value).toBe('api_key');expect(access.text()).toContain('OpenRouter (отдельный доступ)')
   expect(field(wrapper,'Модель').findAll('option').map(o=>o.attributes('value'))).toEqual(['moondream/parakeet-ultra','nvidia/nemotron-3.5-transcribe']);expect(field(wrapper,'API-ключ OpenRouter').find('input').exists()).toBe(true)
   await access.setValue('local');expect((field(wrapper,'Модель').find('select').element as HTMLSelectElement).value).toBe('');expect(writes).toHaveLength(0)
   await access.setValue('api_key');expect((field(wrapper,'API-ключ OpenRouter').find('input').element as HTMLInputElement).value).toBe('');expect(writes).toHaveLength(0)
  }finally{wrapper.unmount()}
 })
 it('keeps unavailable local providers visible as an actionable state and blocks saving an invented model',async()=>{
  const {wrapper,writes}=await render('analysis',{provider:'opencode',model:'other-tenant/model',base_url:'http://model-runner:4096'},new Error('offline'));try{
   expect(wrapper.text()).toContain('Локальные модели недоступны');expect(wrapper.findAll('label').some(l=>l.text().startsWith('API-ключ'))).toBe(false)
   await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toHaveLength(0);expect(wrapper.find('[role="alert"]').text()).toContain('локальн')
  }finally{wrapper.unmount()}
 })
 it('keeps local model selection readonly for a reviewer',async()=>{
  const {wrapper,writes}=await render('asr',{provider:'opencode',credential_source:'local',local_provider:'apinex',model:'apinex/voice-stt',base_url:'http://model-runner:4096'},localCatalog,'reviewer');try{
   expect(field(wrapper,'Доступ к моделям').find('select').attributes('disabled')).toBeDefined();expect(field(wrapper,'Модель').find('select').attributes('disabled')).toBeDefined();await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toHaveLength(0)
  }finally{wrapper.unmount()}
 })
})
