<script setup lang="ts">
import {computed,ref} from 'vue'
import type {Filter} from '../../shared/interfaces'
import {fields,operatorsFor,typedValue} from '../../shared/lib/filter'
import Icon from '../../shared/ui/Icon.vue'
const props=withDefaults(defineProps<{modelValue:Filter;depth?:number}>(),{depth:0})
const emit=defineEmits<{(event:'update:modelValue',value:Filter):void}>()
const error=ref('')
const group=computed(()=>props.modelValue.op==='and'||props.modelValue.op==='or')
const predicate=computed(()=>('field'in props.modelValue?props.modelValue:{field:'direction',op:'eq',value:''}))
function update(next:Filter){error.value='';emit('update:modelValue',next)}
function setMode(mode:string){if(mode==='and'||mode==='or')update({op:mode,rules:'rules'in props.modelValue?props.modelValue.rules:[props.modelValue]});else if(mode==='not')update({op:'not',rule:props.modelValue.op==='not'&&'rule'in props.modelValue?props.modelValue.rule:{op:'and',rules:[]}})}
function addGroup(){if('rules'in props.modelValue)update({...props.modelValue,rules:[...props.modelValue.rules,{op:'and',rules:[]}]})}
function addRule(){if('rules'in props.modelValue)update({...props.modelValue,rules:[...props.modelValue.rules,{field:'direction',op:'eq',value:'in'}]})}
function child(index:number,value?:Filter){if('rules'in props.modelValue){const rules=[...props.modelValue.rules];value?rules.splice(index,1,value):rules.splice(index,1);update({...props.modelValue,rules})}}
function defaultValue(field:string){return ['score','duration'].includes(field)?'0':['has_recording','human_connected'].includes(field)?'true':field==='started_at'?new Date().toISOString():''}
function setField(field:string){changePredicate(field,field==='tag_ids'?'contains':'eq',defaultValue(field))}
function setOperator(op:string){const field=predicate.value.field;changePredicate(field,op,predicate.value.op==='is_null'?defaultValue(field):rawValue.value)}
function changePredicate(field:string,op:string,value:unknown){try{update({field,op,value:typeof value==='string'?typedValue(field,op,value):value})}catch(e){error.value=e instanceof Error?e.message:'Некорректное значение'}}
const rawValue=computed(()=>Array.isArray(predicate.value.value)?predicate.value.value.join(', '):predicate.value.value===undefined?'':String(predicate.value.value))
</script>
<template>
 <div v-if="group" class="filter-group"><div class="group-head"><select aria-label="Логика группы" :value="modelValue.op" @change="setMode(($event.target as HTMLSelectElement).value)"><option value="and">Все условия (И)</option><option value="or">Любое условие (ИЛИ)</option><option v-if="depth<5" value="not">Отрицание (НЕ)</option></select><button type="button" class="button small" @click="addRule"><Icon name="plus" :size="12"/>Условие</button><button v-if="depth<4" type="button" class="text-link" @click="addGroup">+ Группа</button><slot name="remove"/></div><div v-if="'rules'in modelValue" class="filter-children"><div v-for="(rule,i) in modelValue.rules" :key="i" class="filter-row"><FilterBuilder :model-value="rule" :depth="depth+1" @update:model-value="child(i,$event)"><template #remove><button type="button" class="icon-button" aria-label="Удалить группу" @click="child(i)"><Icon name="close" :size="13"/></button></template></FilterBuilder><button v-if="'field'in rule" type="button" class="icon-button" aria-label="Удалить условие" @click="child(i)"><Icon name="close" :size="13"/></button></div></div><p v-if="'rules'in modelValue&&!modelValue.rules.length" class="subtle">Без условий — все звонки.</p></div>
 <div v-else-if="modelValue.op==='not'&&'rule'in modelValue" class="filter-group"><div class="group-head"><span class="badge">НЕ</span><slot name="remove"/></div><FilterBuilder :model-value="modelValue.rule" :depth="depth+1" @update:model-value="update({op:'not',rule:$event})"/></div>
 <div v-else class="filter-row"><select aria-label="Поле фильтра" :value="predicate.field" @change="setField(($event.target as HTMLSelectElement).value)"><option v-for="[value,label] in fields" :key="value" :value="value">{{label}}</option></select><select aria-label="Оператор" :value="predicate.op" @change="setOperator(($event.target as HTMLSelectElement).value)"><option v-for="[value,label] in operatorsFor(predicate.field)" :key="value" :value="value">{{label}}</option></select><select v-if="['has_recording','human_connected'].includes(predicate.field)&&predicate.op!=='is_null'" :value="String(predicate.value)" @change="changePredicate(predicate.field,predicate.op,($event.target as HTMLSelectElement).value)"><option value="true">Да</option><option value="false">Нет</option></select><input v-else-if="predicate.op!=='is_null'" aria-label="Значение фильтра" :value="rawValue" :placeholder="predicate.op==='in'?'Значения через запятую':'Значение'" :type="['score','duration'].includes(predicate.field)&&predicate.op!=='in'?'number':'text'" @change="changePredicate(predicate.field,predicate.op,($event.target as HTMLInputElement).value)"/><span v-if="error" class="subtle" style="color:#b35c55">{{error}}</span></div>
</template>
