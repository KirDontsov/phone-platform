import type {Me} from '../interfaces'
export class ApiError extends Error {constructor(public code:string,message:string,public status=0){super(message)}}
let csrf='',organization='',generation=0
const active=new Set<AbortController>()
export function resetContext(){generation++;for(const controller of active)controller.abort();active.clear();csrf='';organization=''}
export function setContext(me:Me){csrf=me.csrf_token;organization=me.organization.id}
export function currentGeneration(){return generation}
export async function request<T>(path:string,options:{method?:string;body?:unknown;public?:boolean;suppressUnauthorizedEvent?:boolean;signal?:AbortSignal}={}):Promise<T>{
 const own=new AbortController(),captured=generation;active.add(own)
 const abort=()=>own.abort();options.signal?.addEventListener('abort',abort,{once:true})
 const method=options.method||'GET';const headers:Record<string,string>={'Accept':'application/json'}
 if(options.body!==undefined)headers['Content-Type']='application/json'
 if(!options.public&&csrf&&method!=='GET')headers['X-CSRF-Token']=csrf
 if(!options.public&&organization)headers['X-Organization-Id']=organization
 try{
  const response=await fetch('/api'+path,{method,credentials:'include',cache:'no-store',headers,body:options.body===undefined?undefined:JSON.stringify(options.body),signal:own.signal})
  const envelope=await response.json().catch(()=>null)
  if(captured!==generation)throw new ApiError('stale_context','Контекст компании изменился')
  if(!response.ok){if(response.status===401&&!options.public&&!options.suppressUnauthorizedEvent)window.dispatchEvent(new Event('session-expired'));throw new ApiError(envelope?.error?.code||'request_failed',envelope?.error?.message||'Не удалось выполнить запрос',response.status)}
  if(!envelope||!('data' in envelope))throw new ApiError('invalid_response','Сервер вернул некорректный ответ',response.status)
  return envelope.data as T
 }catch(error){if(own.signal.aborted||captured!==generation)throw new ApiError('aborted','Запрос отменён');if(error instanceof ApiError)throw error;throw new ApiError('network_error','Нет связи с сервером. Проверьте подключение.')}
 finally{active.delete(own);options.signal?.removeEventListener('abort',abort)}
}
export function query(values:Record<string,unknown>){const p=new URLSearchParams();for(const[k,v]of Object.entries(values))if(v!==''&&v!==undefined&&v!==null)p.set(k,typeof v==='object'?JSON.stringify(v):String(v));return p.size?'?'+p.toString():''}
export const isCancelled=(e:unknown)=>e instanceof ApiError&&['aborted','stale_context'].includes(e.code)
export const errorMessage=(e:unknown)=>e instanceof Error?e.message:'Не удалось выполнить действие'
