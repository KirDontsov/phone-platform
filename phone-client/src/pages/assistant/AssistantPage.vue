<script setup lang="ts">
import {onMounted,watch} from 'vue'
import {useRoute,useRouter} from 'vue-router'
import {useAuth} from '../../entities/auth/model'
import {useAssistant} from '../../entities/assistant/model'
import AssistantPanel from '../../features/assistant/AssistantPanel.vue'
const auth=useAuth(),store=useAssistant(),route=useRoute(),router=useRouter()
function fromUrl(){const id=route.query.conversation;if(auth.isOwner&&typeof id==='string'&&id!==store.conversation?.id)store.selectConversation(id)}
watch(()=>route.query.conversation,fromUrl)
watch(()=>store.conversation?.id,id=>{if(!auth.isOwner||store.loading)return;const current=typeof route.query.conversation==='string'?route.query.conversation:undefined;if(id!==current)router.replace({path:'/assistant',query:id?{conversation:id}:{}})})
onMounted(fromUrl)
</script>
<template><div v-if="auth.isOwner" class="assistant-page"><AssistantPanel :key="auth.me?.organization.id"/></div><div v-else class="card"><div class="card-body"><h1>Помощник доступен владельцу</h1><p>Настройками компании управляет её владелец.</p><RouterLink class="text-link" to="/dashboard">Вернуться к обзору</RouterLink></div></div></template>
