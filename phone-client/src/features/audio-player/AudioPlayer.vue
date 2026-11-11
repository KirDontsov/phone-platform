<script setup lang="ts">
import {ref,watch} from 'vue'
import type {Recording} from '../../shared/interfaces'
import Icon from '../../shared/ui/Icon.vue'
const props=defineProps<{callId:string;recordings:Recording[];seekTo?:number;seekVersion?:number}>(),audio=ref<HTMLAudioElement|null>(null),active=ref(0),error=ref('')
const pendingSeek=ref<number>()
function jump(){if(!audio.value||props.seekTo===undefined)return;if(audio.value.readyState===0){pendingSeek.value=props.seekTo;return}audio.value.currentTime=props.seekTo;pendingSeek.value=undefined;audio.value.play().catch(()=>{})}
watch(()=>[props.seekTo,props.seekVersion],jump)
function retry(){error.value='';audio.value?.load()}
watch(()=>props.callId,()=>{active.value=0;error.value=''})
function audioError(){error.value='Запись недоступна или ещё готовится. Обновите карточку и попробуйте снова.'}
</script>
<template><div class="audio-box"><div class="audio-caption"><span style="display:flex;gap:7px;align-items:center"><Icon name="play" :size="13"/>{{recordings[active]?.synthetic===true?'Тестовая запись':'Запись разговора'}}</span><select v-if="recordings.length>1" v-model="active" aria-label="Выбрать запись"><option v-for="(recording,i) in recordings" :key="recording.id" :value="i">Запись {{i+1}}</option></select><span v-else>{{recordings.length?'Доступ только вашей компании':'Запись отсутствует'}}</span></div><audio v-if="recordings[active]" ref="audio" controls preload="metadata" :key="recordings[active]!.id" :src="`/api/calls/${callId}/media/${recordings[active]!.id}`" @error="audioError" @loadedmetadata="pendingSeek!==undefined&&jump()"></audio><p v-if="recordings[active]?.synthetic===true" class="subtle" style="margin:8px 0 0">Синтетический звук для проверки плеера. Демонстрационная расшифровка сформирована отдельно.</p><p v-else-if="!recordings[active]" class="subtle" style="margin:0">Разговор не записывался либо файл ещё не поступил из АТС. Звонок сохранён в истории.</p><p v-if="error" class="subtle" style="color:#b56e64;margin:9px 0 0">{{error}} <button class="text-link" @click="retry">Повторить загрузку</button></p></div></template>
