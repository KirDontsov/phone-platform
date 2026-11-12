import {afterEach,describe,expect,it,vi} from 'vitest'
import {flushPromises,mount,type VueWrapper} from '@vue/test-utils'
import {createPinia,setActivePinia} from 'pinia'
import {createMemoryHistory,createRouter} from 'vue-router'
import AuthPage from '../src/pages/auth/AuthPage.vue'
import applicationRouter from '../src/router'
import {useAuth} from '../src/entities/auth/model'
import {request,resetContext,setContext} from '../src/shared/api/client'
import type {Me} from '../src/shared/interfaces'

const identity={user:{id:'new-user',email:'new-owner@example.test',name:'Новый владелец'},organization:{id:'new-company',name:'Новая компания',timezone:'Europe/Moscow',callback_minutes:120,retention_days:90},organizations:[{id:'new-company',name:'Новая компания',role:'owner'}],role:'owner',csrf_token:'new-csrf',demo_mode:false} as Me
const pending={verification_required:true,email:identity.user.email,retry_after:60,expires_in:900}
const payload={name:'Новый владелец',organization_name:'Новая компания',email:'new-owner@example.test',password:'Synthetic-Register-2026'}
const wrappers:VueWrapper[]=[]
afterEach(()=>{for(const wrapper of wrappers.splice(0))wrapper.unmount();resetContext();sessionStorage.clear();vi.unstubAllGlobals()})
function response(data:unknown,status=201){return new Response(JSON.stringify({data}),{status,headers:{'Content-Type':'application/json'}})}
async function render(path='/register'){
 const pinia=createPinia();setActivePinia(pinia);const auth=useAuth();auth.ready=true
 const router=createRouter({history:createMemoryHistory(),routes:[{path:'/login',component:AuthPage},{path:'/register',component:AuthPage},{path:'/reset',component:AuthPage},{path:'/invite/accept',component:AuthPage},{path:'/verify-email',component:{template:'<div>Подтверждение почты</div>'}},{path:'/dashboard',component:{template:'<div>Рабочее пространство</div>'}}]})
 await router.push(path);await router.isReady();const wrapper=mount(AuthPage,{global:{plugins:[pinia,router]}});wrappers.push(wrapper);await flushPromises();return{wrapper,router,auth}
}
function input(wrapper:VueWrapper,label:string){const field=wrapper.findAll('label').find(l=>l.text()===label);expect(field,`Missing field ${label}`).toBeDefined();return field!.get('input')}
async function fill(wrapper:VueWrapper,confirmation=payload.password){
 await input(wrapper,'Ваше имя').setValue(payload.name);await input(wrapper,'Название компании').setValue(payload.organization_name);await input(wrapper,'Рабочая почта').setValue(payload.email);await input(wrapper,'Пароль').setValue(payload.password);await input(wrapper,'Повторите пароль').setValue(confirmation)
}

describe('public account registration',()=>{
 it('exposes registration from login and as a public application route',async()=>{
  const {wrapper}=await render('/login');expect(wrapper.get('a[href="/register"]').text()).toBe('Зарегистрироваться');expect(applicationRouter.getRoutes().find(route=>route.path==='/register')?.meta.public).toBe(true);expect(applicationRouter.getRoutes().find(route=>route.path==='/verify-email')?.meta.public).toBe(true)
 })
 it('submits only identity fields, waits for emailed verification and never adopts202 as an authenticated session',async()=>{
  const {wrapper,router,auth}=await render();setContext({...identity,organization:{...identity.organization,id:'old-company'},csrf_token:'old-csrf'})
  const fetcher=vi.fn(async(url:string)=>response(url==='/api/auth/register'?pending:{items:[]},url==='/api/auth/register'?202:200));vi.stubGlobal('fetch',fetcher)
  expect(wrapper.get('h2').text()).toBe('Создать аккаунт');await fill(wrapper)
  expect(input(wrapper,'Название компании').attributes('maxlength')).toBe('160');expect(input(wrapper,'Ваше имя').attributes('maxlength')).toBe('120');expect(input(wrapper,'Пароль').attributes('minlength')).toBe('10');expect(input(wrapper,'Пароль').attributes('autocomplete')).toBe('new-password')
  await wrapper.get('form').trigger('submit');await flushPromises();expect(fetcher).toHaveBeenCalledTimes(1)
  const[url,options]=fetcher.mock.calls[0]! as unknown as[string,RequestInit];expect(url).toBe('/api/auth/register');expect(JSON.parse(String(options.body))).toEqual(payload);expect(options.method).toBe('POST');expect(options.credentials).toBe('include');expect(options.headers).not.toHaveProperty('X-Organization-Id');expect(options.headers).not.toHaveProperty('X-CSRF-Token')
  expect(auth.me).toBeNull();expect(router.currentRoute.value.path).toBe('/verify-email');expect(auth.pendingEmail).toBe(payload.email);expect(sessionStorage.getItem('phone.pending-registration-email')).toBe(payload.email);expect(Object.values(sessionStorage)).not.toContain(payload.password);await request('/calls');expect((fetcher.mock.calls[1]! as unknown as[string,RequestInit])[1].headers).not.toHaveProperty('X-Organization-Id')
 })
 it('keeps mismatched passwords on the form without sending a registration request',async()=>{
  const{wrapper,router}=await render();const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);await fill(wrapper,'Different-Synthetic-Password');await wrapper.get('form').trigger('submit');await flushPromises();expect(fetcher).not.toHaveBeenCalled();expect(wrapper.get('[role="alert"]').text()).toContain('Пароли не совпадают');expect(router.currentRoute.value.path).toBe('/register')
 })
 it('does not let unrelated or previous invitation tokens hide registration and login email fields',async()=>{
  const registration=await render('/register?token=unrelated-token');expect(input(registration.wrapper,'Рабочая почта').element.tagName==='INPUT').toBe(true)
  const previous=await render('/invite/accept?token=synthetic-invite');await previous.router.push('/register');await flushPromises();expect(input(previous.wrapper,'Рабочая почта').element.tagName==='INPUT').toBe(true);await previous.router.push('/login');await flushPromises();expect(input(previous.wrapper,'Рабочая почта').element.tagName==='INPUT').toBe(true)
 })
 it('blocks double submission until the actual server response completes',async()=>{
  const{wrapper,auth}=await render();let complete!:(value:Response)=>void;const fetcher=vi.fn(()=>new Promise<Response>(resolve=>complete=resolve));vi.stubGlobal('fetch',fetcher);await fill(wrapper);await wrapper.get('form').trigger('submit');expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined();await wrapper.get('form').trigger('submit');expect(fetcher).toHaveBeenCalledTimes(1);complete(response(pending,202));await flushPromises();expect(auth.busy).toBe(false);expect(auth.me).toBeNull();expect(auth.pendingEmail).toBe(payload.email)
 })
 it('shows duplicate account errors, keeps the draft and does not invent a successful session',async()=>{
  const{wrapper,router,auth}=await render();vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({error:{code:'email_registered',message:'Этот адрес уже зарегистрирован. Войдите в аккаунт.'}}),{status:409,headers:{'Content-Type':'application/json'}})));await fill(wrapper);await wrapper.get('form').trigger('submit');await flushPromises();expect(wrapper.get('[role="alert"]').text()).toContain('Этот адрес уже зарегистрирован');expect(auth.me).toBeNull();expect(router.currentRoute.value.path).toBe('/register');expect((input(wrapper,'Рабочая почта').element as HTMLInputElement).value).toBe(payload.email);expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeUndefined()
 })
 it('never claims that mail was sent when SMTP delivery fails',async()=>{
  const{wrapper,router,auth}=await render();vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({error:{code:'mail_unavailable',message:'Почтовый сервис недоступен'}}),{status:503,headers:{'Content-Type':'application/json'}})));await fill(wrapper);await wrapper.get('form').trigger('submit');await flushPromises();expect(wrapper.get('[role="alert"]').text()).toContain('Почтовый сервис недоступен');expect(auth.me).toBeNull();expect(auth.pendingEmail).toBe('');expect(sessionStorage.length).toBe(0);expect(router.currentRoute.value.path).toBe('/register')
 })
 it('rejects an old immediate-login response instead of treating it as email verification',async()=>{
  const{wrapper,router,auth}=await render();vi.stubGlobal('fetch',vi.fn(async()=>response(identity,201)));await fill(wrapper);await wrapper.get('form').trigger('submit');await flushPromises();expect(auth.me).toBeNull();expect(wrapper.get('[role="alert"]').text()).toContain('Сервер не подтвердил отправку письма');expect(router.currentRoute.value.path).toBe('/register');expect(sessionStorage.length).toBe(0)
 })
 it('preserves invitation acceptance and password reset without registration company fields',async()=>{
  const invite=await render('/invite/accept?token=synthetic-invite');const requests:{url:string;body:unknown}[]=[];vi.stubGlobal('fetch',vi.fn(async(url:string,options:RequestInit)=>{requests.push({url,body:JSON.parse(String(options.body))});return response(identity,200)}))
  expect(invite.wrapper.get('h2').text()).toBe('Присоединиться к команде');expect(invite.wrapper.text()).not.toContain('Название компании');await input(invite.wrapper,'Ваше имя').setValue('Коллега');await input(invite.wrapper,'Пароль').setValue(payload.password);await invite.wrapper.get('form').trigger('submit');await flushPromises();expect(requests).toEqual([{url:'/api/auth/invitations/accept',body:{token:'synthetic-invite',name:'Коллега',password:payload.password}}])
  const reset=await render('/reset');requests.length=0;await input(reset.wrapper,'Рабочая почта').setValue(payload.email);await reset.wrapper.get('form').trigger('submit');await flushPromises();expect(requests).toEqual([{url:'/api/auth/reset/request',body:{email:payload.email}}]);expect(reset.wrapper.text()).toContain('Если адрес зарегистрирован');expect(reset.wrapper.text()).not.toContain('Название компании')
 })
})
