import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import SettingsPage from '../src/pages/settings/SettingsPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
async function render(config:Record<string,unknown>){
 const pinia=createPinia();setActivePinia(pinia);useAuth().adopt({user:{id:'u',email:'owner@example.test',name:'Owner'},organization:{id:'o',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'o',name:'Company',role:'owner'}],role:'owner',csrf_token:'csrf',demo_mode:true} as Me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/settings/:tab',component:{template:'<div/>'}}]});await router.push('/settings/telphin');await router.isReady();const writes:{url:string;body:any}[]=[]
 vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='PUT'){writes.push({url,body:JSON.parse(String(options.body))});config={...config,...writes.at(-1)!.body.config}}return new Response(JSON.stringify({data:{items:[{id:'conn',kind:'telphin',state:'configured',config,last_error:null,last_sync_at:null}]}}),{headers:{'Content-Type':'application/json'}})}))
 const wrapper=mount(SettingsPage,{global:{plugins:[pinia,router]}});await flushPromises();return{wrapper,writes}
}
describe('Telphin history boundary settings',()=>{
 it('starts unverified, explains limited search and submits only explicit user confirmation',async()=>{
  const{wrapper,writes}=await render({host:'apiproxy.telphin.ru',client_id:'123',app_id:'app',app_secret:'<configured>'});try{
   const check=wrapper.findAll('label').find(l=>l.text()==='Граница истории сверена с АТС')!.find('input');const date=wrapper.find('input[type="date"]');expect((check.element as HTMLInputElement).checked).toBe(false);expect((date.element as HTMLInputElement).value).toBe('');expect(wrapper.text()).toContain('проверяет последний год; полнота остаётся неподтверждённой');expect(wrapper.text()).toContain('до указанной даты начала истории')
   await date.setValue('2025-03-01');await check.setValue(true);await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toHaveLength(1);expect(writes[0]!.url).toBe('/api/connections/telphin');expect(writes[0]!.body.config).toMatchObject({history_start:'2025-03-01',history_start_verified:true,app_secret:''});expect(wrapper.text()).toContain('Подключение сохранено')
  }finally{wrapper.unmount()}
 })
 it('loads an API boundary date without treating its presence as verification',async()=>{
  const{wrapper}=await render({history_start:'2024-01-02T00:00:00Z',history_start_verified:false});try{const check=wrapper.findAll('label').find(l=>l.text()==='Граница истории сверена с АТС')!.find('input');expect((wrapper.find('input[type="date"]').element as HTMLInputElement).value).toBe('2024-01-02');expect((check.element as HTMLInputElement).checked).toBe(false)}finally{wrapper.unmount()}
 })
 it('loads existing extension mapping and saves string IDs/numbers; empty mapping clears it',async()=>{
  const{wrapper,writes}=await render({extension_mapping:{'12345':'101'}});try{const input=wrapper.find('textarea');expect((input.element as HTMLTextAreaElement).value).toContain('"12345": "101"');await input.setValue('{"12345":"101","67890":"202"}');await wrapper.find('form').trigger('submit');await flushPromises();expect(writes[0]!.body.config.extension_mapping).toEqual({'12345':'101','67890':'202'});await input.setValue('');await wrapper.find('form').trigger('submit');await flushPromises();expect(writes[1]!.body.config.extension_mapping).toEqual({})}finally{wrapper.unmount()}
 })
 it('rejects malformed, array and non-string extension maps before any API write',async()=>{
  const{wrapper,writes}=await render({});try{for(const invalid of ['not json','[]','null','{"12345":101}','{"12345":""}','{"":"101"}']){await wrapper.find('textarea').setValue(invalid);await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toHaveLength(0);expect(wrapper.find('[role="alert"]').text()).toMatch(/JSON/)} }finally{wrapper.unmount()}
 })

})
