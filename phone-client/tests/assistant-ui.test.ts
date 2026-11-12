import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {createRouter,createMemoryHistory} from 'vue-router'
import AssistantPanel from '../src/features/assistant/AssistantPanel.vue'
import AssistantFab from '../src/features/assistant/AssistantFab.vue'
import AssistantPage from '../src/pages/assistant/AssistantPage.vue'
import {useAssistant} from '../src/entities/assistant/model'
import {resetContext} from '../src/shared/api/client'
import {assistantPinia,assistantMe,disconnected,conversation,response} from './assistant-fixtures'
let cleanup:(()=>void)|undefined
afterEach(()=>{cleanup?.();cleanup=undefined;resetContext();vi.unstubAllGlobals()})
async function setup(){const{pinia,auth}=assistantPinia();const router=createRouter({history:createMemoryHistory(),routes:[{path:'/:pathMatch(.*)*',component:{template:'<div/>'}}]});await router.push('/dashboard');await router.isReady();const store=useAssistant();cleanup=()=>store.reset();return{pinia,auth,router,store}}
describe('assistant interface boundaries',()=>{
 it('blocks a credential paste before history/message transport and opens the separate secure form',async()=>{
  const{pinia,router,store}=await setup();const calls:string[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string)=>{calls.push(url);return response(url.endsWith('/status')?disconnected:{items:[],next_cursor:null})}));const wrapper=mount(AssistantPanel,{global:{plugins:[pinia,router]}});try{await flushPromises();const event=new Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(event,'clipboardData',{value:{getData:()=>'{"app_secret":"PASTE_PRIVACY_SENTINEL"}'}});wrapper.find('textarea[aria-label="Задача помощнику"]').element.dispatchEvent(event);await flushPromises();expect(event.defaultPrevented).toBe(true);expect(wrapper.find('[aria-label="Защищённая форма доступов Телфина"]').exists()).toBe(true);expect(wrapper.text()).not.toContain('PASTE_PRIVACY_SENTINEL');expect(store.messages).toEqual([]);expect(calls.some(url=>url.endsWith('/messages'))).toBe(false)}finally{wrapper.unmount()}
 })
 it('uses CSRF-safe server history to select and rename a conversation without losing its version',async()=>{
  const{pinia,router,store}=await setup();const writes:any[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='PATCH'){writes.push(JSON.parse(String(options.body)));return response({...conversation,title:'Проверка настроек',version:2})}if(url.endsWith('/status'))return response(disconnected);if(url.includes('/conversations/'))return response({conversation,messages:[],runs:[],credentials:[]});return response({items:[conversation],next_cursor:null})}));const wrapper=mount(AssistantPanel,{global:{plugins:[pinia,router]}});try{await flushPromises();await wrapper.find('.assistant-conversation-button').trigger('click');await flushPromises();expect(store.conversation?.id).toBe(conversation.id);await wrapper.find('button[aria-label="Переименовать диалог"]').trigger('click');await wrapper.find('.assistant-history-edit input').setValue('Проверка настроек');await wrapper.find('.assistant-history-edit').trigger('submit');await flushPromises();expect(writes).toEqual([{title:'Проверка настроек',version:1}]);expect(store.conversation?.version).toBe(2)}finally{wrapper.unmount()}
 })
 it('clears an open secret form when the company changes and never loads assistant for a reviewer',async()=>{
  const{pinia,router,auth}=await setup();vi.stubGlobal('fetch',vi.fn(async(url:string)=>response(url.endsWith('/status')?disconnected:{items:[],next_cursor:null})));const wrapper=mount(AssistantPanel,{global:{plugins:[pinia,router]}});try{await flushPromises();await wrapper.findAll('button').find(b=>b.text().includes('Добавить доступы'))!.trigger('click');await wrapper.find('input[type="password"]').setValue('SWITCH_PRIVACY_SENTINEL');auth.adopt({...structuredClone(assistantMe),organization:{...assistantMe.organization,id:'org-2',name:'Компания Б'}});await flushPromises();expect(wrapper.find('input[type="password"]').exists()).toBe(false);expect(wrapper.text()).toContain('Компания Б');expect(wrapper.text()).not.toContain('SWITCH_PRIVACY_SENTINEL')}finally{wrapper.unmount()}
  auth.adopt({...structuredClone(assistantMe),role:'reviewer'});const fetch=vi.fn();vi.stubGlobal('fetch',fetch);const fab=mount(AssistantFab,{global:{plugins:[pinia,router]}}),page=mount(AssistantPage,{global:{plugins:[pinia,router]}});expect(fab.find('button').exists()).toBe(false);expect(page.text()).toContain('Помощник доступен владельцу');expect(fetch).not.toHaveBeenCalled();fab.unmount();page.unmount()
 })
})
