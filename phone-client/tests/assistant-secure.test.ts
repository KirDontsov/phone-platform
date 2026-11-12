import {afterEach,describe,it,expect,vi} from 'vitest'
import {mount,flushPromises} from '@vue/test-utils'
import {resetContext} from '../src/shared/api/client'
import {containsCredentialText,parseTelphinCredentials} from '../src/shared/interfaces/assistant'
import SecureCredentialsForm from '../src/features/assistant/SecureCredentialsForm.vue'
import ModelAccessCard from '../src/features/assistant/ModelAccessCard.vue'
import {assistantPinia,conversation,credential,disconnected,response} from './assistant-fixtures'
afterEach(()=>{resetContext();vi.unstubAllGlobals()})
describe('assistant secret channels',()=>{
 it('accepts only a strict Telphin object and does not echo invalid input',()=>{
  for(const invalid of ['[]','null','{"app_id":"x","app_secret":"PRIVACY_SENTINEL","client_id":123}','{"app_id":"x","app_secret":"PRIVACY_SENTINEL","client_id":"y","extra":true}']){try{parseTelphinCredentials(invalid);throw Error('should reject')}catch(e){expect(String(e)).not.toContain('PRIVACY_SENTINEL')}}expect(parseTelphinCredentials('{"app_id":"1","app_secret":"key","client_id":"2"}').client_id).toBe('2');expect(containsCredentialText('Как подключить Телфин?')).toBe(false);expect(containsCredentialText('app_secret=PRIVATE')).toBe(true)
 })
 it('saves App ID and App Secret with no Client ID in both fields and JSON',async()=>{
  for(const jsonMode of [false,true]){
   const{pinia}=assistantPinia();const writes:{url:string;body:any}[]=[];
   vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{writes.push({url,body:JSON.parse(String(options.body))});return response(url.endsWith('/credentials')?credential:conversation)}));
   const wrapper=mount(SecureCredentialsForm,{global:{plugins:[pinia]}});
   try{
    if(jsonMode){await wrapper.findAll('button').find(b=>b.text()==='Вставить JSON')!.trigger('click');await wrapper.find('textarea').setValue('{"app_id":"app","app_secret":"PRIVACY_SENTINEL"}')}
    else{await wrapper.findAll('label').find(l=>l.text()==='App ID')!.find('input').setValue('app');await wrapper.find('input[type="password"]').setValue('PRIVACY_SENTINEL');const id=wrapper.find('input[inputmode="numeric"]');expect(id.attributes('required')).toBeUndefined();expect((wrapper.find('form').element as HTMLFormElement).checkValidity()).toBe(true)}
    await wrapper.find('form').trigger('submit');await flushPromises();
    expect(writes.map(w=>w.url)).toEqual(['/api/assistant/conversations','/api/assistant/conversations/conversation-1/credentials']);
    expect(writes[1]!.body.config).toMatchObject({app_id:'app',app_secret:'PRIVACY_SENTINEL'});expect(writes[1]!.body.config).not.toHaveProperty('client_id');expect(JSON.stringify(wrapper.emitted('saved'))).not.toContain('PRIVACY_SENTINEL');
   }finally{wrapper.unmount();resetContext()}
  }
 })
 it('accepts a missing or blank Client ID and preserves an explicit numeric ID',()=>{
  for(const input of [{app_id:'app',app_secret:'secret'},{app_id:'app',app_secret:'secret',client_id:''}])expect(parseTelphinCredentials(JSON.stringify(input))).toEqual({app_id:'app',app_secret:'secret'});
  expect(parseTelphinCredentials('{"app_id":"app","app_secret":"secret","client_id":"42"}').client_id).toBe('42');
 })
 it('sends pasted JSON only to the credential endpoint, emits a summary and clears on success/remount',async()=>{
  const{pinia}=assistantPinia();const writes:{url:string;body:any}[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{writes.push({url,body:JSON.parse(String(options.body))});return response(url.endsWith('/credentials')?credential:conversation)}));const wrapper=mount(SecureCredentialsForm,{global:{plugins:[pinia]}});try{await wrapper.findAll('button').find(b=>b.text()==='Вставить JSON')!.trigger('click');await wrapper.find('textarea').setValue('{"app_id":"app","app_secret":"PRIVACY_SENTINEL","client_id":"12345"}');await wrapper.find('form').trigger('submit');await flushPromises();expect(writes.map(w=>w.url)).toEqual(['/api/assistant/conversations','/api/assistant/conversations/conversation-1/credentials']);expect(writes[1]!.body.config.app_secret).toBe('PRIVACY_SENTINEL');expect(JSON.stringify(wrapper.emitted('saved'))).not.toContain('PRIVACY_SENTINEL');expect((wrapper.find('textarea').element as HTMLTextAreaElement).value).toBe('')}finally{wrapper.unmount()}const next=mount(SecureCredentialsForm,{global:{plugins:[pinia]}});expect((next.find('input[type="password"]').element as HTMLInputElement).value).toBe('');next.unmount()
 })
 it('rejects secret form errors without reflecting server error bodies',async()=>{
  const{pinia}=assistantPinia();vi.stubGlobal('fetch',vi.fn(async(url:string)=>url.endsWith('/credentials')?response({code:'bad',message:'PRIVACY_SENTINEL'},400):response(conversation)));const wrapper=mount(SecureCredentialsForm,{global:{plugins:[pinia]}});try{await wrapper.findAll('button').find(b=>b.text()==='Вставить JSON')!.trigger('click');await wrapper.find('textarea').setValue('{"app_id":"app","app_secret":"PRIVACY_SENTINEL","client_id":"12345"}');await wrapper.find('form').trigger('submit');await flushPromises();expect(wrapper.find('[role="alert"]').text()).not.toContain('PRIVACY_SENTINEL')}finally{wrapper.unmount()}
 })
 it('sends the model API key exclusively through auth/start and clears the field',async()=>{
  const writes:{url:string;body:any}[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='POST'){writes.push({url,body:JSON.parse(String(options.body))});return response({...disconnected,authenticated:true,auth_mode:'apiKey'})}return response(disconnected)}));const wrapper=mount(ModelAccessCard);try{await flushPromises();await wrapper.find('input[type="password"]').setValue('MODEL_PRIVACY_SENTINEL');await wrapper.find('form').trigger('submit');await flushPromises();expect(writes).toEqual([{url:'/api/assistant/auth/start',body:{type:'apiKey',api_key:'MODEL_PRIVACY_SENTINEL'}}]);expect(wrapper.text()).toContain('Вход подтверждён');expect(wrapper.text()).not.toContain('MODEL_PRIVACY_SENTINEL');expect(wrapper.find('input[type="password"]').exists()).toBe(false)}finally{wrapper.unmount()}
 })
 it('refuses a foreign verification URL without claiming successful authentication',async()=>{
  vi.stubGlobal('fetch',vi.fn(async()=>response({...disconnected,pending_login:{login_id:'login',verification_url:'https://example.test/codex/device',user_code:'SAFE-CODE',expires_at:'2099-11-12T12:00:00Z'}})));const wrapper=mount(ModelAccessCard);try{await flushPromises();expect(wrapper.find('a').exists()).toBe(false);expect(wrapper.text()).toContain('неподдерживаемый адрес');expect(wrapper.text()).not.toContain('Вход подтверждён')}finally{wrapper.unmount()}
 })
 it('shows expiry and clears the old code without claiming successful login',async()=>{
  vi.stubGlobal('fetch',vi.fn(async()=>response({...disconnected,pending_login:{login_id:'login',verification_url:'https://auth.openai.com/codex/device',user_code:'EXPIRED-CODE',expires_at:'2020-11-12T12:00:00Z'}})));const wrapper=mount(ModelAccessCard);try{await flushPromises();expect(wrapper.text()).toContain('Срок кода истёк');expect(wrapper.text()).not.toContain('EXPIRED-CODE');expect(wrapper.text()).not.toContain('Вход подтверждён')}finally{wrapper.unmount()}
 })

 it('does not let an older status read undo a newly confirmed login',async()=>{
  let reads=0,release!:(value:Response)=>void;vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{if(options.method==='POST')return response({...disconnected,authenticated:true,auth_mode:'apiKey'});if(++reads===1)return response(disconnected);return new Promise<Response>(resolve=>release=resolve)}));const wrapper=mount(ModelAccessCard);try{await flushPromises();await wrapper.findAll('button').find(b=>b.text()==='Проверить подключение')!.trigger('click');await wrapper.find('input[type="password"]').setValue('MODEL_PRIVACY_SENTINEL');await wrapper.find('form').trigger('submit');await flushPromises();expect(wrapper.text()).toContain('Вход подтверждён');release(response(disconnected));await flushPromises();expect(wrapper.text()).toContain('Вход подтверждён')}finally{wrapper.unmount()}
 })

})
