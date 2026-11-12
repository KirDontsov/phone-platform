<script setup lang="ts">
import {computed,ref,watch} from 'vue'
import type {Recording} from '../../shared/interfaces'
import Icon from '../../shared/ui/Icon.vue'
const props=defineProps<{callId:string;recordings:Recording[];seekTo?:number;seekVersion?:number}>(),audio=ref<HTMLAudioElement|null>(null),active=ref(0),error=ref('')
const pendingSeek=ref<number>()
const ready=(recording:Recording|undefined)=>!!recording&&(recording.ready===true||recording.state==='ready')
const selected=computed(()=>props.recordings[active.value]),available=computed(()=>ready(selected.value)),failed=computed(()=>selected.value?.state==='failed'),rateLimited=computed(()=>selected.value?.last_error==='provider_rate_limited')
let selectedId:string|undefined,manualSelection=false
function choose(){selectedId=selected.value?.id;manualSelection=true;error.value=''}
watch(()=>props.callId,()=>{active.value=0;selectedId=undefined;manualSelection=false;error.value='';pendingSeek.value=undefined})
watch(()=>props.recordings.map(r=>`${r.id}:${ready(r)}:${r.state}`).join('|'),()=>{
 const retained=props.recordings.findIndex(r=>r.id===selectedId)
 const firstReady=props.recordings.findIndex(ready)
 active.value=retained>=0?retained:Math.max(0,firstReady)
 if(!manualSelection&&!ready(selected.value)&&firstReady>=0)active.value=firstReady
 selectedId=selected.value?.id
},{immediate:true})
watch(()=>`${props.callId}:${selected.value?.id}:${available.value}`,()=>{error.value=''})
function jump(){if(!audio.value||props.seekTo===undefined)return;if(audio.value.readyState===0){pendingSeek.value=props.seekTo;return}audio.value.currentTime=props.seekTo;pendingSeek.value=undefined;audio.value.play().catch(()=>{})}
watch(()=>[props.seekTo,props.seekVersion],jump)
function retry(){error.value='';audio.value?.load()}
function audioError(){error.value='Не удалось воспроизвести готовую запись. Повтори загрузку.'}
</script>
<template><div class="audio-box"><div class="audio-caption"><span style="display:flex;gap:7px;align-items:center"><Icon name="play" :size="13"/>{{selected?.synthetic===true?'Тестовая запись':'Запись разговора'}}</span><select v-if="recordings.length>1" v-model="active" aria-label="Выбрать запись" @change="choose"><option v-for="(recording,i) in recordings" :key="recording.id" :value="i">Запись {{i+1}}{{ready(recording)?' · готова':recording.state==='failed'?' · ошибка':' · загружается'}}</option></select><span v-else>{{selected?(available?'Доступ только вашей компании':failed?'Ошибка загрузки':rateLimited?'Ожидание лимита Телфина':'Загружается из Телфина'):'Запись отсутствует'}}</span></div><audio v-if="selected&&available" ref="audio" controls preload="metadata" :key="`${callId}:${selected.id}`" :src="`/api/calls/${callId}/media/${selected.id}`" @error="audioError" @loadedmetadata="pendingSeek!==undefined&&jump()"></audio><p v-else-if="selected&&failed" class="subtle" role="status" style="margin:0">Не удалось загрузить запись из Телфина. Подробности — в разделе «Обработка и аудит».</p><p v-else-if="selected&&rateLimited" class="subtle" role="status" style="margin:0">Телфин временно ограничил загрузку записей. Система автоматически повторит попытку после восстановления лимита.</p><p v-else-if="selected" class="subtle" role="status" style="margin:0">Запись загружается из Телфина. Плеер появится автоматически, когда файл будет готов.</p><p v-else class="subtle" style="margin:0">Разговор не записывался либо файл ещё не поступил из АТС. Звонок сохранён в истории.</p><p v-if="selected?.synthetic===true&&available" class="subtle" style="margin:8px 0 0">Синтетический звук для проверки плеера. Демонстрационная расшифровка сформирована отдельно.</p><p v-if="error&&available" class="subtle" style="color:#b56e64;margin:9px 0 0">{{error}} <button class="text-link" @click="retry">Повторить загрузку</button></p></div></template>
