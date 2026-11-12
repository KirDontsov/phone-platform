import {defineStore} from 'pinia'
import {ref,computed} from 'vue'
import type {Me} from '../../shared/interfaces'
import {ApiError,request,resetContext,setContext,errorMessage,isCancelled} from '../../shared/api/client'
export interface RegistrationInput {name:string;email:string;password:string;organization_name:string}
export interface PendingRegistration {verification_required:true;email:string;retry_after:number;expires_in:number}
export interface VerificationResend {message:string;retry_after:number}
const PENDING_EMAIL='phone.pending-registration-email'
function readPendingEmail(){try{return sessionStorage.getItem(PENDING_EMAIL)||''}catch{return''}}
export const useAuth=defineStore('auth',()=>{
 const me=ref<Me|null>(null),ready=ref(false),busy=ref(false),error=ref(''),errorCode=ref(''),pendingEmail=ref(readPendingEmail()),verificationRetryAt=ref(0)
 const canWrite=computed(()=>!!me.value&&me.value.role!=='reviewer'),isOwner=computed(()=>me.value?.role==='owner')
 function adopt(value:Me){me.value=value;setContext(value);ready.value=true}
 async function load(){try{adopt(await request<Me>('/auth/me',{suppressUnauthorizedEvent:true}))}catch(e){if(!isCancelled(e)){me.value=null;resetContext()}}finally{ready.value=true}}
 function setPendingEmail(email:string,retryAfter=0){pendingEmail.value=email.trim().toLowerCase();verificationRetryAt.value=Date.now()+Math.max(0,Math.min(retryAfter,3600))*1000;try{sessionStorage.setItem(PENDING_EMAIL,pendingEmail.value)}catch{}}
 function clearPendingEmail(){pendingEmail.value='';verificationRetryAt.value=0;try{sessionStorage.removeItem(PENDING_EMAIL)}catch{}}
 function failed(e:unknown){error.value=errorMessage(e);errorCode.value=e instanceof ApiError?e.code:''}
 async function login(email:string,password:string){busy.value=true;error.value='';errorCode.value='';try{resetContext();adopt(await request<Me>('/auth/login',{method:'POST',body:{email,password},public:true}));clearPendingEmail();return true}catch(e){failed(e);return false}finally{busy.value=false}}
 async function register(input:RegistrationInput):Promise<PendingRegistration|null>{if(busy.value)return null;busy.value=true;error.value='';errorCode.value='';try{resetContext();me.value=null;const body={name:input.name.trim(),email:input.email.trim(),password:input.password,organization_name:input.organization_name.trim()};const pending=await request<PendingRegistration>('/auth/register',{method:'POST',body,public:true});if(pending.verification_required!==true||!pending.email||!Number.isFinite(pending.retry_after)||!Number.isFinite(pending.expires_in))throw new Error('Сервер не подтвердил отправку письма');setPendingEmail(pending.email,pending.retry_after);return pending}catch(e){failed(e);return null}finally{busy.value=false}}
 async function verifyEmail(email:string,code:string,password?:string){if(busy.value)return false;busy.value=true;error.value='';errorCode.value='';try{resetContext();const body={email:email.trim(),code:code.trim(),...(password?{password}:{})};const verified=await request<Me>('/auth/verify-email',{method:'POST',body,public:true});if(!verified?.user?.id||!verified?.organization?.id||!verified?.csrf_token)throw new Error('Сервер не подтвердил вход в аккаунт');adopt(verified);clearPendingEmail();return true}catch(e){failed(e);return false}finally{busy.value=false}}
 async function resendVerification(email:string):Promise<VerificationResend|null>{if(busy.value)return null;busy.value=true;error.value='';errorCode.value='';try{const result=await request<VerificationResend>('/auth/resend-verification',{method:'POST',body:{email:email.trim()},public:true});setPendingEmail(email,result.retry_after);return result}catch(e){failed(e);return null}finally{busy.value=false}}
 async function logout(){try{await request('/auth/logout',{method:'POST',body:{}})}finally{resetContext();me.value=null;ready.value=true}}
 async function switchOrganization(id:string){if(id===me.value?.organization.id)return;busy.value=true;error.value='';try{const next=await request<Me>('/auth/organization',{method:'POST',body:{organization_id:id}});resetContext();adopt(next)}catch(e){error.value=errorMessage(e);throw e}finally{busy.value=false}}
 function expire(){resetContext();me.value=null;ready.value=true}
 return{me,ready,busy,error,errorCode,pendingEmail,verificationRetryAt,canWrite,isOwner,load,login,register,verifyEmail,resendVerification,setPendingEmail,clearPendingEmail,logout,switchOrganization,adopt,expire}
})
