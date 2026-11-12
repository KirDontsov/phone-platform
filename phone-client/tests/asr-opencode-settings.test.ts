import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import SettingsPage from '../src/pages/settings/SettingsPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
async function render(config:Record<string,unknown>,tab='asr',role='owner'){
 const pinia=createPinia();setActivePinia(pinia);useAuth().adopt({user:{id:'u',email:'owner@example.test',name:'Owner'},organization:{id:'o',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'o',name:'Company',role}],role,csrf_token:'csrf',demo_mode:true} as Me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/settings/:tab',component:{template:'<div/>'}}]});await router.push('/settings/'+tab);await router.isReady();const writes:{url:string;body:any}[]=[]
 vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method!=='GET'){const body=JSON.parse(String(options.body));writes.push({url,body});if(options.method==='PUT'){config={...config,...body.config};if(body.config.api_key)config.api_key='<configured>'}}const data=url.endsWith('/local-models')?{available:true,providers:[{id:'vendor',name:'Existing provider',models:[{id:'vendor/text-model',name:'Text model'}]}],asr_models:[]}:url.endsWith('/test')?{ok:true,message:'Доступ OpenRouter проверен; распознавание не запускалось'}:{items:[{id:'conn',kind:tab,state:'configured',config,last_error:null,last_sync_at:null}]};return new Response(JSON.stringify({data}),{headers:{'Content-Type':'application/json'}})}))
 const wrapper=mount(SettingsPage,{global:{plugins:[pinia,router]}});await flushPromises();return{wrapper,writes}
}
function field(wrapper:ReturnType<typeof mount>,name:string){return wrapper.findAll('label').find(l=>l.text().startsWith(name))!}
describe('OpenCode ASR settings',()=>{
 it('adds Docker OpenCode and exact model choices, saves per-tenant OpenRouter key without changing config on load',async()=>{
  const {wrapper,writes}=await render({provider:'openai',model:'configured-openai-model',base_url:'https://api.openai.com/v1',api_key:'<configured>'});try{
   expect(writes).toHaveLength(0);await field(wrapper,'API-ключ').find('input').setValue('synthetic-old-provider-key');const provider=field(wrapper,'Провайдер').find('select');expect(provider.findAll('option').map(o=>o.attributes('value'))).toContain('opencode');await provider.setValue('opencode');await field(wrapper,'Доступ к моделям').find('select').setValue('api_key');expect((field(wrapper,'API-ключ OpenRouter').find('input').element as HTMLInputElement).value).toBe('');
   const model=field(wrapper,'Модель').find('select');expect(model.findAll('option').map(o=>o.attributes('value'))).toEqual(['moondream/parakeet-ultra','nvidia/nemotron-3.5-transcribe']);
   const base=field(wrapper,'Адрес API / сервера').find('input');expect((base.element as HTMLInputElement).value).toBe('http://model-runner:4096');expect(base.attributes('readonly')).toBeDefined();
   await field(wrapper,'API-ключ OpenRouter').find('input').setValue('synthetic-provider-key');await model.setValue('nvidia/nemotron-3.5-transcribe');await wrapper.find('form').trigger('submit');await flushPromises();
   expect(writes).toEqual([{url:'/api/connections/asr',body:{config:{provider:'opencode',model:'nvidia/nemotron-3.5-transcribe',base_url:'http://model-runner:4096',api_key:'synthetic-provider-key',credential_source:'api_key',app_secret:'',client_secret:'',access_token:'',refresh_token:''}}}]);expect(wrapper.text()).toContain('Туннель и компьютер должны быть включены');expect(wrapper.text()).toContain('OpenRouter оплачивается отдельно');expect(wrapper.html()).not.toContain('synthetic-provider-key')
  }finally{wrapper.unmount()}
 })
 it('blocks provider switching with a blank old secret and retains an existing same-provider key on save',async()=>{
  const {wrapper,writes}=await render({provider:'openai',model:'old',base_url:'https://api.openai.com/v1',api_key:'<configured>'});try{
   await field(wrapper,'Провайдер').find('select').setValue('opencode');await field(wrapper,'Доступ к моделям').find('select').setValue('api_key');await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toHaveLength(0);expect(wrapper.find('[role="alert"]').text()).toContain('новый API-ключ')
  }finally{wrapper.unmount()}
  const saved=await render({provider:'opencode',model:'moondream/parakeet-ultra',base_url:'http://model-runner:4096',api_key:'<configured>'});try{
   const key=field(saved.wrapper,'API-ключ OpenRouter').find('input');expect((key.element as HTMLInputElement).value).toBe('');await saved.wrapper.find('form').trigger('submit');await flushPromises();expect(saved.writes[0]!.body.config).toMatchObject({provider:'opencode',model:'moondream/parakeet-ultra',api_key:''});await saved.wrapper.findAll('button').find(b=>b.text()==='Проверить соединение')!.trigger('click');await flushPromises();expect(saved.writes.at(-1)!.url).toBe('/api/connections/asr/test')
  }finally{saved.wrapper.unmount()}
 })
 it('preserves an available OpenCode analysis model without an API key and prevents reviewer mutations',async()=>{
  const legacy=await render({provider:'opencode',model:'vendor/text-model',base_url:'http://model-runner:4096',api_key:'<configured>'},'analysis');try{
   expect((field(legacy.wrapper,'Модель').find('select').element as HTMLSelectElement).value).toBe('vendor/text-model');expect(legacy.wrapper.findAll('label').some(l=>l.text().startsWith('API-ключ'))).toBe(false);expect(legacy.wrapper.text()).not.toContain('OpenRouter оплачивается отдельно');await legacy.wrapper.find('form').trigger('submit');await flushPromises();expect(legacy.writes[0]!.body.config).toMatchObject({provider:'opencode',model:'vendor/text-model'});expect(legacy.writes[0]!.body.config).not.toHaveProperty('api_key')
  }finally{legacy.wrapper.unmount()}
  const reviewer=await render({provider:'opencode',model:'moondream/parakeet-ultra',base_url:'http://model-runner:4096',api_key:'<configured>'},'asr','reviewer');try{
   expect(field(reviewer.wrapper,'Провайдер').find('select').attributes('disabled')).toBeDefined();expect(field(reviewer.wrapper,'Модель').find('select').attributes('disabled')).toBeDefined();expect(reviewer.wrapper.findAll('button').some(b=>b.text()==='Сохранить')).toBe(false);await reviewer.wrapper.find('form').trigger('submit');await flushPromises();expect(reviewer.writes).toHaveLength(0)
  }finally{reviewer.wrapper.unmount()}
 })
})
