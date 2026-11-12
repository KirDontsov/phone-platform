import {afterEach,describe,expect,it,vi} from 'vitest'
import {flushPromises,mount,type VueWrapper} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createMemoryHistory,createRouter} from 'vue-router'
import VerifyEmailPage from '../src/pages/auth/VerifyEmailPage.vue'
import {useAuth} from '../src/entities/auth/model'
import {request,resetContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'
const me={user:{id:'new-user',email:'new@example.test',name:'Новый владелец'},organization:{id:'new-company',name:'Новая компания',timezone:'Europe/Moscow',callback_minutes:120,retention_days:90},organizations:[{id:'new-company',name:'Новая компания',role:'owner'}],role:'owner',csrf_token:'new-csrf',demo_mode:false} as Me
const wrappers:VueWrapper[]=[]
afterEach(()=>{for(const w of wrappers.splice(0))w.unmount();resetContext();sessionStorage.clear();vi.unstubAllGlobals();vi.useRealTimers()})
const response=(data:unknown,status=200)=>new Response(JSON.stringify({data}),{status,headers:{'Content-Type':'application/json'}})
const failure=(code:string,message:string,status=400)=>new Response(JSON.stringify({error:{code,message}}),{status,headers:{'Content-Type':'application/json'}})
async function render(path='/verify-email',pending=''){
 const pinia=createPinia();setActivePinia(pinia);const auth=useAuth();auth.ready=true;if(pending)auth.setPendingEmail(pending)
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/verify-email',component:VerifyEmailPage},{path:'/login',component:{template:'<div>Вход</div>'}},{path:'/register',component:{template:'<div>Регистрация</div>'}},{path:'/dashboard',component:{template:'<div>Компания</div>'}}]});await router.push(path);await router.isReady();const wrapper=mount(VerifyEmailPage,{global:{plugins:[pinia,router]}});wrappers.push(wrapper);await flushPromises();return{wrapper,router,auth}
}
function input(wrapper:VueWrapper,label:string){return wrapper.findAll('label').find(l=>l.text()===label)!.get('input')}
async function fill(wrapper:VueWrapper){await input(wrapper,'Рабочая почта').setValue(me.user.email);await input(wrapper,'Код из письма').setValue('012345')}
describe('mandatory emailed verification',()=>{
 it('uses email-only reload recovery and adopts an app session only after successful verification',async()=>{
  sessionStorage.setItem('phone.pending-registration-email',me.user.email);const{wrapper,router,auth}=await render();const fetcher=vi.fn(async(url:string)=>response(url.endsWith('verify-email')?me:{items:[]}));vi.stubGlobal('fetch',fetcher)
  expect((input(wrapper,'Рабочая почта').element as HTMLInputElement).value).toBe(me.user.email);expect(auth.me).toBeNull();expect(fetcher).not.toHaveBeenCalled();await input(wrapper,'Код из письма').setValue('012345');await wrapper.get('form').trigger('submit');await flushPromises()
  const[url,options]=fetcher.mock.calls[0]! as unknown as[string,RequestInit];expect(url).toBe('/api/auth/verify-email');expect(JSON.parse(String(options.body))).toEqual({email:me.user.email,code:'012345'});expect(options.credentials).toBe('include');expect(options.headers).not.toHaveProperty('X-Organization-Id');expect(auth.me).toEqual(me);expect(router.currentRoute.value.path).toBe('/dashboard');expect(sessionStorage.length).toBe(0);await request('/calls');expect((fetcher.mock.calls[1]! as unknown as[string,RequestInit])[1].headers).toHaveProperty('X-Organization-Id','new-company')
 })
 it('removes email and code query parameters before auto-submitting an email link',async()=>{
  let query:Record<string,unknown>={};const fetcher=vi.fn(async()=>{expect(query).not.toHaveProperty('code');expect(query).not.toHaveProperty('email');return response(me)});vi.stubGlobal('fetch',fetcher)
  const pinia=createPinia();setActivePinia(pinia);const auth=useAuth();auth.ready=true;const router=createRouter({history:createMemoryHistory(),routes:[{path:'/verify-email',component:VerifyEmailPage},{path:'/register',component:{template:'<div/>'}},{path:'/login',component:{template:'<div/>'}},{path:'/dashboard',component:{template:'<div/>'}}]});await router.push('/verify-email?email=new%40example.test&code=012345');await router.isReady();query=router.currentRoute.value.query;router.afterEach(to=>{query=to.query});const wrapper=mount(VerifyEmailPage,{global:{plugins:[pinia,router]}});wrappers.push(wrapper);await flushPromises();expect(fetcher).toHaveBeenCalledTimes(1);expect(auth.me).toEqual(me);expect(router.currentRoute.value.path).toBe('/dashboard');expect(sessionStorage.length).toBe(0)
 })
 it('requires the signup password when the email link is opened in another browser',async()=>{
  const{wrapper,auth}=await render();const fetcher=vi.fn().mockResolvedValueOnce(failure('verification_password_required','Введите пароль, указанный при регистрации',409)).mockResolvedValueOnce(response(me));vi.stubGlobal('fetch',fetcher);await fill(wrapper);await wrapper.get('form').trigger('submit');await flushPromises();expect(auth.me).toBeNull();expect(wrapper.text()).toContain('Если письмо открыто в другом браузере');await input(wrapper,'Пароль, указанный при регистрации').setValue('Synthetic-Password-Proof');await wrapper.get('form').trigger('submit');await flushPromises();expect(JSON.parse(String(fetcher.mock.calls[1]![1].body))).toEqual({email:me.user.email,code:'012345',password:'Synthetic-Password-Proof'});expect(auth.me).toEqual(me);expect(sessionStorage.length).toBe(0)
 })
 it('never posts incomplete or nonnumeric codes and does not expose a resend success on a missing email',async()=>{
  const{wrapper}=await render();const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);await wrapper.get('form').trigger('submit');await flushPromises();expect(wrapper.get('[role="alert"]').text()).toContain('Укажите почту');await fill(wrapper);await input(wrapper,'Код из письма').setValue('1234x5');await wrapper.get('form').trigger('submit');await flushPromises();expect(wrapper.get('[role="alert"]').text()).toContain('6-значный код');expect(fetcher).not.toHaveBeenCalled()
 })
 it('keeps invalid or expired codes unauthenticated, resends through the server and enforces the returned cooldown',async()=>{
  vi.useFakeTimers();const{wrapper,auth}=await render();const fetcher=vi.fn().mockResolvedValueOnce(failure('verification_invalid','Код неверен или истёк')).mockResolvedValueOnce(response({message:'Если регистрация найдена, новый код отправлен',retry_after:60}));vi.stubGlobal('fetch',fetcher);await fill(wrapper);await wrapper.get('form').trigger('submit');await flushPromises();expect(wrapper.get('[role="alert"]').text()).toContain('Код неверен или истёк');expect(auth.me).toBeNull();await wrapper.get('button[type="button"]').trigger('click');await flushPromises();expect(fetcher.mock.calls[1]![0]).toBe('/api/auth/resend-verification');expect(JSON.parse(String(fetcher.mock.calls[1]![1].body))).toEqual({email:me.user.email});expect(wrapper.get('[role="status"]').text()).toContain('Если регистрация найдена');expect(wrapper.get('button[type="button"]').text()).toContain('60 с');expect(wrapper.get('button[type="button"]').attributes('disabled')).toBeDefined();expect((input(wrapper,'Код из письма').element as HTMLInputElement).value).toBe('');expect(sessionStorage.length).toBe(1);expect(sessionStorage.getItem('phone.pending-registration-email')).toBe(me.user.email);await vi.advanceTimersByTimeAsync(61000);expect(wrapper.get('button[type="button"]').attributes('disabled')).toBeUndefined()
 })
 it('surfaces SMTP failures without pretending that a new code was sent',async()=>{
  const{wrapper,auth}=await render();vi.stubGlobal('fetch',vi.fn(async()=>failure('mail_unavailable','Почтовый сервис недоступен',503)));await fill(wrapper);await wrapper.get('button[type="button"]').trigger('click');await flushPromises();expect(wrapper.get('[role="alert"]').text()).toContain('Почтовый сервис недоступен');expect(wrapper.find('[role="status"]').exists()).toBe(false);expect(auth.me).toBeNull();expect(wrapper.get('button[type="button"]').attributes('disabled')).toBeUndefined();expect((input(wrapper,'Код из письма').element as HTMLInputElement).value).toBe('012345')
 })
})
