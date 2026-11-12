<script setup lang="ts">
import {computed,ref} from 'vue'
import {sheetFields} from '../../shared/lib/sheet-mapping'
const props=defineProps<{modelValue:Record<string,string>;disabled?:boolean}>()
const emit=defineEmits<{(event:'update:modelValue',value:Record<string,string>):void}>()
const field=ref('')
const available=computed(()=>Object.entries(sheetFields).filter(([key])=>!Object.prototype.hasOwnProperty.call(props.modelValue,key)))
function column(key:string,value:string){emit('update:modelValue',{...props.modelValue,[key]:value.trim().toUpperCase()})}
function remove(key:string){const next={...props.modelValue};delete next[key];emit('update:modelValue',next)}
function add(){if(!field.value)return;const used=new Set(Object.values(props.modelValue));let label='';for(let index=1;index<52;index++){const candidate=index<26?String.fromCharCode(65+index):'A'+String.fromCharCode(65+index-26);if(!used.has(candidate)){label=candidate;break}}if(label){emit('update:modelValue',{...props.modelValue,[field.value]:label});field.value=''}}
</script>
<template><div class="sheet-mapping"><h4>Соответствие полей колонкам</h4><p class="token-hint">Одна строка — один звонок. Колонка A содержит стабильный ключ: повторная отправка обновляет результат.</p><div class="mapping-row" v-for="(value,key) in modelValue" :key="key"><label :for="'sheet-column-'+key">{{sheetFields[key]||key}}</label><input :id="'sheet-column-'+key" :aria-label="'Колонка: '+(sheetFields[key]||key)" :value="value" :disabled="disabled||key==='call_id'" maxlength="2" placeholder="B" @input="column(String(key),($event.target as HTMLInputElement).value)"/><button v-if="key!=='call_id'&&!disabled" class="text-link" type="button" :aria-label="'Убрать поле: '+(sheetFields[key]||key)" @click="remove(String(key))">Убрать</button><span v-else class="subtle">Ключ</span></div><div v-if="!disabled&&available.length" class="mapping-add"><select v-model="field" aria-label="Добавить поле выгрузки"><option value="">Выбрать поле</option><option v-for="[key,label] in available" :key="key" :value="key">{{label}}</option></select><button type="button" class="button small" :disabled="!field" @click="add">Добавить поле</button></div></div></template>
<style scoped>.sheet-mapping{margin-top:20px}.mapping-row{display:grid;grid-template-columns:minmax(130px,1fr) 72px 70px;align-items:center;gap:12px;padding:8px 0;border-bottom:1px solid #edf1ef}.mapping-row label{margin:0}.mapping-add{display:flex;gap:12px;margin-top:14px;flex-wrap:wrap}.mapping-add select{flex:1;min-width:120px}</style>
