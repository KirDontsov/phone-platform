import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createRouter,createMemoryHistory} from 'vue-router'
import SettingsPage from '../src/pages/settings/SettingsPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {resetContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
async function render(role='owner'){
 const pinia=createPinia();setActivePinia(pinia)
 const me={user:{id:'u',email:'owner@example.test',name:'Owner'},organization:{id:'o',name:'Company',timezone:'UTC',callback_minutes:120,retention_days:90},organizations:[{id:'o',name:'Company',role}],role,csrf_token:'csrf',demo_mode:true} as Me
 useAuth().adopt(me)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/settings/:tab',component:{template:'<div/>'}}]});await router.push('/settings/organization');await router.isReady()
 const writes:any[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='PATCH'){writes.push(JSON.parse(String(options.body)));Object.assign(me.organization,writes.at(-1))}return new Response(JSON.stringify({data:url==='/api/auth/me'?me:me.organization}),{headers:{'Content-Type':'application/json'}})}))
 const wrapper=mount(SettingsPage,{global:{plugins:[pinia,router]}});await flushPromises();return{wrapper,writes}
}
describe('explicit retention choice',()=>{
 it('keeps automatic deletion off until owner explicitly enables it and explains historical deletion',async()=>{
  const{wrapper,writes}=await render();try{const control=wrapper.get('input[aria-label="Автоматически удалять старые звонки"]');expect((control.element as HTMLInputElement).checked).toBe(false);expect(wrapper.text()).toContain('включая загруженную историю');await control.setValue(true);await wrapper.get('form').trigger('submit');await flushPromises();expect(writes[0].retention_enabled).toBe(true);await control.setValue(false);await wrapper.get('form').trigger('submit');await flushPromises();expect(writes[1].retention_enabled).toBe(false)}finally{wrapper.unmount()}
 })
 it('prevents manager changing automatic deletion policy',async()=>{const{wrapper}=await render('manager');try{expect((wrapper.get('input[aria-label="Автоматически удалять старые звонки"]').element as HTMLInputElement).disabled).toBe(true)}finally{wrapper.unmount()}})
})
