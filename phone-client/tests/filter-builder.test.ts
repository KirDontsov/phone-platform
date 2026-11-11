import {describe,it,expect} from 'vitest'
import {mount} from '@vue/test-utils'
import FilterBuilder from '../src/features/filter-builder/FilterBuilder.vue'
describe('FilterBuilder',()=>{
 it('creates predicates and nested groups through the actual UI',async()=>{const wrapper=mount(FilterBuilder,{props:{modelValue:{op:'and',rules:[]}}});await wrapper.findAll('button').find(b=>b.text().includes('Условие'))!.trigger('click');expect(wrapper.emitted('update:modelValue')?.[0]?.[0]).toEqual({op:'and',rules:[{field:'direction',op:'eq',value:'in'}]});await wrapper.findAll('button').find(b=>b.text().includes('Группа'))!.trigger('click');expect(wrapper.emitted('update:modelValue')?.[1]?.[0]).toEqual({op:'and',rules:[{op:'and',rules:[]}]})})
 it('switches a predicate field to a typed numeric condition',async()=>{const wrapper=mount(FilterBuilder,{props:{modelValue:{field:'employee',op:'eq',value:'A'}}});await wrapper.find('select[aria-label="Поле фильтра"]').setValue('score');expect(wrapper.emitted('update:modelValue')?.[0]?.[0]).toEqual({field:'score',op:'eq',value:0})})
})
