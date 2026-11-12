import {createPinia,setActivePinia} from 'pinia'
import {useAuth} from '../src/entities/auth/model'
import type {Me} from '../src/shared/interfaces'
import type {Conversation,AssistantRun,AssistantMessage,AssistantStatus,CredentialSummary} from '../src/shared/interfaces/assistant'
export const assistantMe:Me={user:{id:'user-1',name:'Owner',email:'owner@example.test'},organization:{id:'org-1',name:'Компания А',timezone:'UTC',callback_minutes:120,retention_days:180},organizations:[{id:'org-1',name:'Компания А',role:'owner'},{id:'org-2',name:'Компания Б',role:'owner'}],role:'owner',csrf_token:'csrf',demo_mode:true}
export const conversation:Conversation={id:'conversation-1',title:'Настройка отдела',version:1,active_run_id:null,created_at:'2026-11-12T12:00:00Z',updated_at:'2026-11-12T12:00:00Z'}
export const run:AssistantRun={id:'run-1',conversation_id:conversation.id,state:'running',assistant_message_id:'message-2',events:[],error_code:null,model:'codex-test-wire',usage:null,started_at:'2026-11-12T12:00:00Z',finished_at:null}
export const userMessage:AssistantMessage={id:'message-1',role:'user',text:'Создай теги',status:'ready',run_id:run.id,created_at:'2026-11-12T12:00:00Z'}
export const assistantMessage:AssistantMessage={id:'message-2',role:'assistant',text:'',status:'running',run_id:run.id,created_at:'2026-11-12T12:00:00Z'}
export const disconnected:AssistantStatus={available:true,runner_ready:true,authenticated:false,auth_mode:null,codex_version:'test',model:null,error_code:'assistant_auth_required',pending_login:null}
export const credential:CredentialSummary={id:'credential-1',kind:'telphin',label:'Доступы Телфина',created_at:'2026-11-12T12:00:00Z',expires_at:'2099-11-13T12:00:00Z'}
export const response=(data:unknown,status=200)=>new Response(JSON.stringify(status>=400?{error:data}:{data}),{status,headers:{'Content-Type':'application/json'}})
export function assistantPinia(){const pinia=createPinia();setActivePinia(pinia);const auth=useAuth();auth.adopt(structuredClone(assistantMe));return{pinia,auth}}
