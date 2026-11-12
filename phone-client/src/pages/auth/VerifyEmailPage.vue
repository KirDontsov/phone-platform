<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref} from 'vue'
import {useRoute,useRouter} from 'vue-router'
import {useAuth} from '../../entities/auth/model'
import Icon from '../../shared/ui/Icon.vue'
import Notice from '../../shared/ui/Notice.vue'
const auth=useAuth(),route=useRoute(),router=useRouter()
const fromQuery=typeof route.query.email==='string'?route.query.email:''
const linkCode=typeof route.query.code==='string'?route.query.code:''
const email=ref(fromQuery||auth.pendingEmail),code=ref(linkCode),password=ref(''),passwordRequired=ref(false),error=ref(''),message=ref(''),busy=ref(false),resending=ref(false),cooldown=ref(0)
let timer:ReturnType<typeof setInterval>|undefined
const waiting=computed(()=>busy.value||resending.value||auth.busy)
function updateCooldown(){cooldown.value=Math.max(0,Math.ceil((auth.verificationRetryAt-Date.now())/1000))}
function changeEmail(){code.value='';password.value='';passwordRequired.value=false;error.value='';message.value=''}
async function submit(){
 if(waiting.value)return
 error.value='';message.value=''
 if(!email.value.trim()){error.value='Укажите почту, выбранную при регистрации';return}
 if(!/^\d{6}$/.test(code.value.trim())){error.value='Введите 6-значный код из письма';return}
 if(passwordRequired.value&&!password.value){error.value='Введите пароль, указанный при регистрации';return}
 busy.value=true
 try{
  if(await auth.verifyEmail(email.value,code.value,passwordRequired.value?password.value:undefined)){code.value='';password.value='';await router.replace('/dashboard')}
  else{error.value=auth.error;if(auth.errorCode==='verification_password_required')passwordRequired.value=true}
 }finally{busy.value=false}
}
async function resend(){
 if(waiting.value||cooldown.value>0)return
 error.value='';message.value=''
 if(!email.value.trim()){error.value='Укажите почту, выбранную при регистрации';return}
 resending.value=true
 try{
  const result=await auth.resendVerification(email.value)
  if(result){message.value=result.message;code.value='';password.value='';passwordRequired.value=false}
  else{error.value=auth.error;if(auth.errorCode==='verification_resend_throttled')auth.verificationRetryAt=Date.now()+60000}
  updateCooldown()
 }finally{resending.value=false}
}
onMounted(async()=>{
 if(email.value&&email.value!==auth.pendingEmail)auth.setPendingEmail(email.value)
 updateCooldown();timer=setInterval(updateCooldown,1000)
 // Keep the email code only in component memory. Remove it from the address
 // before the verification POST, including when the email link auto-submits.
 const query={...route.query};delete query.code;delete query.email
 if('code'in route.query||'email'in route.query)await router.replace({path:route.path,query})
 if(/^\d{6}$/.test(linkCode)&&email.value.trim())await submit()
})
onUnmounted(()=>{if(timer)clearInterval(timer)})
</script>
<template>
 <div class="auth-layout">
  <section class="auth-story">
   <div class="brand"><span class="brand-symbol"><i></i><i></i><i></i><i></i></span><span>Сигнал<small>АНАЛИТИКА ЗВОНКОВ</small></span></div>
   <div><span class="eyebrow">ВАШЕ РАБОЧЕЕ ПРОСТРАНСТВО</span><h1>Остался<br><em>один шаг.</em></h1><p>Подтвердите почту, чтобы открыть компанию и подключить телефонию.</p></div>
   <small>Каждая компания видит только свои данные.</small>
  </section>
  <section class="auth-form-area">
   <form class="auth-card" @submit.prevent="submit">
    <div class="auth-icon"><Icon name="lock" :size="25"/></div>
    <h2>Подтвердите почту</h2><p class="muted">Введите 6-значный код из письма. Код действует 15 минут.</p>
    <Notice :message="error" kind="error"/><Notice :message="message" kind="success"/>
    <label>Рабочая почта<input v-model="email" name="email" type="email" autocomplete="email" maxlength="254" required placeholder="name@company.ru" @input="changeEmail"/></label>
    <label>Код из письма<input v-model="code" name="code" type="text" inputmode="numeric" autocomplete="one-time-code" minlength="6" maxlength="6" pattern="[0-9]{6}" required placeholder="000000"/></label>
    <template v-if="passwordRequired"><p class="subtle">Если письмо открыто в другом браузере, введите также пароль регистрации.</p><label>Пароль, указанный при регистрации<input v-model="password" name="verification_password" type="password" autocomplete="current-password" maxlength="512" required/></label></template>
    <button type="submit" class="button primary full" :disabled="waiting">{{busy?'Подтверждаем…':'Подтвердить почту'}}<Icon name="arrow" :size="17"/></button>
    <button type="button" class="button full" :disabled="waiting||cooldown>0" @click="resend">{{resending?'Отправляем…':cooldown>0?`Отправить снова через ${cooldown} с`:'Отправить код ещё раз'}}</button>
    <p class="subtle">Для повторной отправки кода используйте браузер, в котором регистрировались, или начните регистрацию заново.</p>
    <RouterLink class="text-link" to="/register">Вернуться к регистрации</RouterLink><RouterLink class="text-link" to="/login">Войти в аккаунт</RouterLink>
    <div class="auth-security"><Icon name="lock" :size="14"/>Защищённая сессия · Изоляция компаний</div>
   </form>
  </section>
 </div>
</template>
