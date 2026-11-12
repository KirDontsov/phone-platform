import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import SettingsPage from '../src/pages/settings/SettingsPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
async function render(tab:string,config:Record<string,unknown>={}){
 const pinia=createPinia();setActivePinia(pinia);useAuth().adopt({user:{id:'u',email:'owner@example.test',name:'Owner'},organization:{id:'o',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'o',name:'Company',role:'owner'}],role:'owner',csrf_token:'csrf',demo_mode:true} as Me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/settings/:tab',component:{template:'<div/>'}}]});await router.push('/settings/'+tab);await router.isReady();const writes:{url:string;body:any}[]=[]
 vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method!=='GET'){const body=JSON.parse(String(options.body));writes.push({url,body});if(options.method==='PUT')config={...config,...body.config}}const data=url==='/api/rubrics'?options.method==='POST'?{id:'rubric-new'}:{items:[]}:url.includes('/activate')?{}:{items:[{id:'conn',kind:tab,state:'configured',config,last_error:null,last_sync_at:null}]};return new Response(JSON.stringify({data}),{headers:{'Content-Type':'application/json'}})}))
 const wrapper=mount(SettingsPage,{global:{plugins:[pinia,router]}});await flushPromises();return{wrapper,writes}
}
describe('customer criteria and Google Sheets setup',()=>{
 it('adds editable checklist examples and saves immutable instructions with explicit evaluation mode',async()=>{
  const{wrapper,writes}=await render('rubrics');try{
   await wrapper.findAll('button').find(b=>b.text()==='Добавить примеры')!.trigger('click');
   expect(wrapper.findAll('input').some(i=>(i.element as HTMLInputElement).value==='Приглашение на семинар')).toBe(true);expect(wrapper.findAll('select[aria-label="Способ оценки"]')).toHaveLength(3)
   await wrapper.find('textarea[aria-label="Инструкция: Приветствие"]').setValue('Проверить представление сотрудника и компании')
   await wrapper.find('form').trigger('submit');await flushPromises();
   const saved=writes.find(w=>w.url==='/api/rubrics')!.body;expect(saved.criteria).toHaveLength(3);expect(saved.criteria[0]).toMatchObject({name:'Приветствие',evaluation_mode:'checklist',instruction:'Проверить представление сотрудника и компании'});expect(writes.at(-1)!.url).toContain('/activate')
  }finally{wrapper.unmount()}
 })
 it('edits field-to-column mapping without JSON and blocks duplicate columns before saving',async()=>{
  const{wrapper,writes}=await render('sheets',{column_mapping:{call_id:'A',summary:'B',criteria:'C'}});try{
   expect(wrapper.find('textarea').exists()).toBe(false);expect(wrapper.text()).toContain('Одна таблица и один лист на компанию');
   await wrapper.get('input[aria-label="Колонка: Итоги разговора"]').setValue('D')
   await wrapper.find('form').trigger('submit');await flushPromises();
   expect(writes[0]!.body.config.column_mapping).toEqual({call_id:'A',summary:'D',criteria:'C'})
   await wrapper.get('input[aria-label="Колонка: Проверка требований"]').setValue('D');await wrapper.find('form').trigger('submit');await flushPromises();
   expect(writes).toHaveLength(1);expect(wrapper.find('[role="alert"]').text()).toContain('разные колонки')
  }finally{wrapper.unmount()}
 })
})
