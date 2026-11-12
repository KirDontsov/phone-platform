<script setup lang="ts">
import {computed,ref,onMounted,watch} from 'vue'
import {useRoute,useRouter} from 'vue-router'
import {useAuth} from '../../entities/auth/model'
import {request,errorMessage} from '../../shared/api/client'
import type {Me} from '../../shared/interfaces'
import Icon from '../../shared/ui/Icon.vue'
import Notice from '../../shared/ui/Notice.vue'
const auth=useAuth(),route=useRoute(),router=useRouter()
const email=ref(''),password=ref(''),confirmation=ref(''),name=ref(''),company=ref(''),error=ref(''),message=ref(''),busy=ref(false),existingAccount=ref(false)
const invite=computed(()=>route.path.startsWith('/invite')),reset=computed(()=>route.path==='/reset'),registration=computed(()=>route.path==='/register')
function readAuthToken(){return invite.value||reset.value?String(route.query.token||''):''}
const token=ref(readAuthToken())
function stripToken(){if(token.value){const query={...route.query};delete query.token;router.replace({path:route.path,query})}}
onMounted(stripToken)
watch(()=>route.path,()=>{token.value=readAuthToken();existingAccount.value=false;password.value='';confirmation.value='';error.value='';message.value='';stripToken()})
const title=computed(()=>registration.value?'Создать аккаунт':invite.value?'Присоединиться к команде':reset.value?(token.value?'Новый пароль':'Восстановить доступ'):'Добро пожаловать')
const description=computed(()=>registration.value?'Создайте свою компанию и подключите телефонию.':invite.value?'Укажите имя и пароль для доступа к компании. Если аккаунт уже существует, используйте текущий пароль.':reset.value?'Вернём вам доступ к рабочему пространству.':'Войдите в рабочее пространство вашей компании.')
const newPassword=computed(()=>registration.value||reset.value||invite.value&&!existingAccount.value)
async function submit(){
 if(busy.value||auth.busy)return
 error.value='';message.value=''
 if(registration.value&&password.value!==confirmation.value){error.value='Пароли не совпадают';return}
 if(registration.value&&(!name.value.trim()||!company.value.trim())){error.value='Укажите имя и название компании';return}
 busy.value=true
 try{
  if(registration.value){const pending=await auth.register({name:name.value,email:email.value,password:password.value,organization_name:company.value});if(pending){password.value='';confirmation.value='';await router.replace({path:'/verify-email',query:{email:pending.email}})}else error.value=auth.error}
  else if(invite.value){const me=await request<Me>('/auth/invitations/accept',{method:'POST',public:true,body:{token:token.value,password:password.value,name:name.value}});auth.adopt(me);await router.replace('/dashboard')}
  else if(reset.value){await request('/auth/reset/'+(token.value?'confirm':'request'),{method:'POST',public:true,body:token.value?{token:token.value,password:password.value}:{email:email.value}});message.value=token.value?'Пароль изменён. Войдите с новым паролем.':'Если адрес зарегистрирован, письмо для восстановления отправлено.';password.value=''}
  else if(await auth.login(email.value,password.value)){const target=String(route.query.redirect||'/dashboard');await router.replace(target.startsWith('/')&&!target.startsWith('//')?target:'/dashboard')}else error.value=auth.error
 }catch(e){error.value=errorMessage(e)}finally{busy.value=false}
}
</script>
<template>
 <div class="auth-layout">
  <section class="auth-story">
   <div class="brand"><span class="brand-symbol"><i></i><i></i><i></i><i></i></span><span>Сигнал<small>АНАЛИТИКА ЗВОНКОВ</small></span></div>
   <div><span class="eyebrow">РАЗГОВОРЫ, КОТОРЫЕ ДВИГАЮТ БИЗНЕС</span><h1>Услышьте больше.<br><em>Поймите главное.</em></h1><p>Звонки, качество диалогов и следующие шаги — в одном рабочем пространстве.</p><div class="auth-wave"><i v-for="n in 35" :key="n" :style="{height:`${18+Math.abs(Math.sin(n*.63))*70}px`}"></i></div></div>
   <small>Каждая компания видит только свои данные.</small>
  </section>
  <section class="auth-form-area">
   <form class="auth-card" @submit.prevent="submit">
    <div class="auth-icon"><Icon :name="invite||registration?'user':reset?'lock':'phone'" :size="25"/></div>
    <h2>{{title}}</h2><p class="muted">{{description}}</p>
    <Notice :message="error" kind="error"/><Notice :message="message" kind="success"/>
    <label v-if="registration||invite&&!existingAccount">Ваше имя<input v-model="name" name="name" autocomplete="name" required maxlength="120"/></label>
    <label v-if="registration">Название компании<input v-model="company" name="organization_name" autocomplete="organization" required maxlength="160"/></label>
    <label v-if="!invite&&!token">Рабочая почта<input v-model="email" name="email" type="email" autocomplete="email" placeholder="name@company.ru" required maxlength="254"/></label>
    <label v-if="invite" class="checkbox"><input v-model="existingAccount" type="checkbox"/>У меня уже есть аккаунт</label>
    <label v-if="!reset||token">{{invite&&existingAccount?'Текущий пароль':'Пароль'}}<input v-model="password" name="password" type="password" :autocomplete="newPassword?'new-password':'current-password'" :minlength="newPassword?10:1" maxlength="512" required :placeholder="newPassword?'Минимум 10 символов':'Введите пароль'"/></label>
    <label v-if="registration">Повторите пароль<input v-model="confirmation" name="password_confirmation" type="password" autocomplete="new-password" minlength="10" maxlength="512" required placeholder="Введите пароль ещё раз"/></label>
    <button type="submit" class="button primary full" :disabled="busy||auth.busy">{{busy?'Подождите…':registration?'Зарегистрироваться':invite?'Присоединиться':reset?(token?'Сохранить пароль':'Отправить письмо'):'Войти'}}<Icon name="arrow" :size="17"/></button>
    <template v-if="!reset&&!invite&&!registration"><RouterLink class="text-link" to="/register">Зарегистрироваться</RouterLink><RouterLink v-if="auth.pendingEmail" class="text-link" :to="{path:'/verify-email',query:{email:auth.pendingEmail}}">Подтвердить почту</RouterLink><RouterLink class="text-link" to="/reset">Не помню пароль</RouterLink></template>
    <RouterLink v-else class="text-link" to="/login">{{registration?'Уже есть аккаунт? Войти':'Вернуться ко входу'}}</RouterLink>
    <p v-if="invite&&!token" class="notice error">В ссылке отсутствует токен приглашения. Обратитесь к владельцу компании.</p>
    <div class="auth-security"><Icon name="lock" :size="14"/>Защищённая сессия · Изоляция компаний</div>
   </form>
  </section>
 </div>
</template>
