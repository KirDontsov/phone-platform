<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref} from 'vue'
import {RouterView,useRoute,useRouter} from 'vue-router'
import {useAuth} from './entities/auth/model'
import Icon from './shared/ui/Icon.vue'
import Notice from './shared/ui/Notice.vue'
import AssistantFab from './features/assistant/AssistantFab.vue'
const auth=useAuth(),route=useRoute(),router=useRouter(),mobile=ref(false),switchError=ref('')
const nav=[{to:'/dashboard',icon:'chart',label:'Обзор'},{to:'/calls',icon:'phone',label:'Все звонки'},{to:'/callbacks',icon:'clock',label:'Контроль перезвонов'},{to:'/integrations',icon:'link',label:'Интеграции'},{to:'/operations',icon:'activity',label:'Обработка и аудит'}]
const visibleNav=computed(()=>auth.isOwner?[...nav,{to:'/assistant',icon:'sparkles',label:'Помощник'}]:nav)
const title=computed(()=>route.path==='/assistant'?'Помощник настройки':route.path.startsWith('/calls/')?'Карточка звонка':route.path.startsWith('/settings')?'Настройки':nav.find(n=>route.path.startsWith(n.to))?.label||'Рабочее пространство')
async function changeOrganization(event:Event){const picker=event.target as HTMLSelectElement;try{switchError.value='';await auth.switchOrganization((event.target as HTMLSelectElement).value);mobile.value=false;await router.replace('/dashboard')}catch{switchError.value=auth.error}finally{picker.value=auth.me?.organization.id||''}}
async function logout(){await auth.logout();await router.replace('/login')}
function expired(){auth.expire();if(!route.meta.public)router.replace('/login')}
onMounted(()=>window.addEventListener('session-expired',expired));onUnmounted(()=>window.removeEventListener('session-expired',expired))
</script>
<template>
 <RouterView v-if="route.meta.public" :key="route.path"/>
 <div v-else-if="auth.me" class="app-shell">
  <button v-if="mobile" class="mobile-shade" @click="mobile=false" aria-label="Закрыть меню"></button>
  <aside class="sidebar" :class="{open:mobile}">
   <RouterLink to="/dashboard" class="brand"><span class="brand-symbol"><i></i><i></i><i></i><i></i></span><span>Сигнал<small>АНАЛИТИКА ЗВОНКОВ</small></span></RouterLink>
   <div class="workspace-picker"><span class="overline">РАБОЧЕЕ ПРОСТРАНСТВО</span><label class="organization-select"><span class="org-avatar">{{auth.me.organization.name.slice(0,1)}}</span><select aria-label="Компания" :value="auth.me.organization.id" :disabled="auth.busy" @change="changeOrganization"><option v-for="org in auth.me.organizations" :key="org.id" :value="org.id">{{org.name}}</option></select></label></div>
   <nav aria-label="Главная навигация"><RouterLink v-for="item in visibleNav" :key="item.to" :to="item.to" @click="mobile=false"><Icon :name="item.icon"/><span>{{item.label}}</span></RouterLink></nav>
   <div class="sidebar-bottom"><div class="sidebar-note"><span class="live-dot"></span><div>Каждый разговор — в фокусе<small>Данные изолированы по компании</small></div></div><RouterLink class="settings-link" to="/settings" @click="mobile=false"><Icon name="settings"/>Настройки</RouterLink><div class="user-card"><span class="avatar">{{(auth.me.user.name||auth.me.user.email).slice(0,1).toUpperCase()}}</span><span class="user-info"><b>{{auth.me.user.name||auth.me.user.email}}</b><small>{{({owner:'Владелец',manager:'Менеджер',reviewer:'Наблюдатель'})[auth.me.role]}}</small></span><button class="icon-button" @click="logout" aria-label="Выйти"><Icon name="logout" :size="18"/></button></div></div>
  </aside>
  <div class="main-shell">
   <header class="topbar"><div class="breadcrumbs"><button class="icon-button mobile-menu" @click="mobile=true" aria-label="Открыть меню"><Icon name="menu"/></button><span>{{auth.me.organization.name}}</span><Icon name="chevron" :size="13"/><b>{{title}}</b></div><div class="topbar-right"><span v-if="auth.me.demo_mode" class="badge amber">Демонстрационный режим</span><span class="timezone">{{auth.me.organization.timezone}}</span><span class="avatar small">{{auth.me.user.name?.slice(0,1)||'Я'}}</span></div></header>
   <main class="workspace"><Notice :message="switchError" kind="error"/><RouterView :key="`${auth.me.organization.id}:${route.path}`"/></main>
   <footer class="workspace-footer"><span>Сигнал · Рабочее пространство команды</span><span>История и оценки доступны в пределах ваших прав</span></footer>
  </div>
  <AssistantFab/>
 </div>
 <div v-else class="boot-screen">Загружаем рабочее пространство…</div>
</template>
