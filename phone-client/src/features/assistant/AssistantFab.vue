<script setup lang="ts">
import {computed,nextTick,onUnmounted,ref,watch} from 'vue'
import {useRoute,useRouter} from 'vue-router'
import {useAuth} from '../../entities/auth/model'
import {useAssistant} from '../../entities/assistant/model'
import Icon from '../../shared/ui/Icon.vue'
import AssistantPanel from './AssistantPanel.vue'
const route=useRoute(),router=useRouter(),auth=useAuth(),store=useAssistant(),open=ref(false),button=ref<HTMLElement|null>(null),dialog=ref<HTMLElement|null>(null),inertTargets=new Map<HTMLElement,boolean>()
const visible=computed(()=>auth.isOwner&&!route.meta.public&&route.path!=='/assistant')
function restore(){for(const[element,value]of inertTargets)element.inert=value;inertTargets.clear()}
async function close(){open.value=false;restore();await nextTick();button.value?.focus()}
function launch(){if(window.innerWidth<768){router.push('/assistant');return}open.value=true}
function trap(event:KeyboardEvent){if(!open.value)return;if(event.key==='Escape'){event.preventDefault();close()}if(event.key==='Tab'){const items=[...dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled),a[href],input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]')||[]].filter(e=>e.getClientRects().length);const first=items[0],last=items.at(-1);if(!first){event.preventDefault();return}if(event.shiftKey&&(document.activeElement===first||document.activeElement===dialog.value)){event.preventDefault();last?.focus()}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus()}}}
watch(open,async value=>{if(value){await nextTick();if(!open.value||!visible.value)return;for(const target of document.querySelectorAll<HTMLElement>('.main-shell,.sidebar')){inertTargets.set(target,target.inert);target.inert=true}dialog.value?.focus();window.addEventListener('keydown',trap)}else{restore();window.removeEventListener('keydown',trap)}})
watch(()=>route.path,()=>{open.value=false});watch(()=>[auth.me?.organization.id,auth.me?.user.id,auth.me?.role].join(':'),()=>{open.value=false},{flush:'sync'})
onUnmounted(()=>{restore();window.removeEventListener('keydown',trap)})
</script>
<template><button v-if="visible&&!open" ref="button" class="assistant-fab" aria-label="Открыть помощника настройки" @click="launch"><Icon name="sparkles" :size="23"/><span>Помощник</span><i v-if="store.busy" class="assistant-fab-busy" aria-label="Задача выполняется"></i></button><div v-if="open&&visible" class="assistant-drawer-shade" @click.self="close"><aside ref="dialog" class="assistant-drawer" role="dialog" aria-modal="true" aria-label="Помощник настройки" tabindex="-1"><AssistantPanel variant="drawer" @close="close"/></aside></div></template>
