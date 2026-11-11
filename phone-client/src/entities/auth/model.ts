import {defineStore} from 'pinia'
import {ref,computed} from 'vue'
import type {Me} from '../../shared/interfaces'
import {request,resetContext,setContext,errorMessage,isCancelled} from '../../shared/api/client'
export const useAuth=defineStore('auth',()=>{
 const me=ref<Me|null>(null),ready=ref(false),busy=ref(false),error=ref('')
 const canWrite=computed(()=>!!me.value&&me.value.role!=='reviewer'),isOwner=computed(()=>me.value?.role==='owner')
 function adopt(value:Me){me.value=value;setContext(value);ready.value=true}
 async function load(){try{adopt(await request<Me>('/auth/me',{suppressUnauthorizedEvent:true}))}catch(e){if(!isCancelled(e)){me.value=null;resetContext()}}finally{ready.value=true}}
 async function login(email:string,password:string){busy.value=true;error.value='';try{resetContext();adopt(await request<Me>('/auth/login',{method:'POST',body:{email,password},public:true}));return true}catch(e){error.value=errorMessage(e);return false}finally{busy.value=false}}
 async function logout(){try{await request('/auth/logout',{method:'POST',body:{}})}finally{resetContext();me.value=null;ready.value=true}}
 async function switchOrganization(id:string){if(id===me.value?.organization.id)return;busy.value=true;error.value='';try{const next=await request<Me>('/auth/organization',{method:'POST',body:{organization_id:id}});resetContext();adopt(next)}catch(e){error.value=errorMessage(e);throw e}finally{busy.value=false}}
 function expire(){resetContext();me.value=null;ready.value=true}
 return{me,ready,busy,error,canWrite,isOwner,load,login,logout,switchOrganization,adopt,expire}
})
