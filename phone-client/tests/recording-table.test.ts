import {describe,it,expect} from 'vitest'
import {mount} from '@vue/test-utils'
import {createRouter,createMemoryHistory} from 'vue-router'
import CallTable from '../src/features/call-table/CallTable.vue'
import type {Call,RecordingState} from '../src/shared/interfaces'
function call(id:string,extra:Partial<Call>&{recording_state?:RecordingState}={}):Call{return{id,provider_id:id,started_at:'2026-11-12T12:00:00Z',phone:'synthetic-'+id,contact_name:'Учебный звонок',employee:'Тестовый сотрудник',direction:'in',provider_result:'bridged',human_connected:true,duration:20,stage:'ready',status_id:null,tag_ids:[],contact_kind:'unknown',score:null,has_recording:true,note:'',manual_lock:false,version:1,coverage_complete:false,...extra} as Call}
const router=createRouter({history:createMemoryHistory(),routes:[{path:'/:pathMatch(.*)*',component:{template:'<div/>'}},{path:'/calls/:id',component:{template:'<div/>'}}]})
describe('recording availability in call tables',()=>{
 it('shows a separate recording column with textual availability for every server state',()=>{
  const states:RecordingState[]=['ready','pending','rate_limited','failed','none','unknown'],labels=['Можно слушать','Загружается','Лимит Телфина','Ошибка загрузки','Нет записи','Не загружена'];const wrapper=mount(CallTable,{props:{calls:states.map(state=>call(state,{recording_state:state,has_recording:state!=='none'}))},global:{plugins:[router]}})
  try{const headers=wrapper.findAll('th').map(h=>h.text()),index=headers.indexOf('Запись');expect(index).toBeGreaterThanOrEqual(0);const rows=wrapper.findAll('tbody tr');for(let i=0;i<states.length;i++){const cell=rows[i]!.findAll('td')[index]!;expect(cell.text()).toBe(labels[i]);expect(cell.find('svg').exists()).toBe(true)}}finally{wrapper.unmount()}
 })
 it('never advertises provider presence or a ready analysis stage as playable media for legacy calls',()=>{
  const wrapper=mount(CallTable,{props:{calls:[call('provider-only'),call('without-recording',{has_recording:false}),call('provider-unset',{has_recording:undefined as unknown as boolean}),call('provider-null',{has_recording:null as unknown as boolean})]},global:{plugins:[router]}})
  try{const index=wrapper.findAll('th').findIndex(h=>h.text()==='Запись');expect(index).toBeGreaterThanOrEqual(0);const rows=wrapper.findAll('tbody tr');expect(rows[0]!.findAll('td')[index]!.text()).toBe('Не загружена');expect(rows[1]!.findAll('td')[index]!.text()).toBe('Нет записи');expect(rows[2]!.findAll('td')[index]!.text()).toBe('Не загружена');expect(rows[3]!.findAll('td')[index]!.text()).toBe('Не загружена');expect(wrapper.text()).not.toContain('Можно слушать')}finally{wrapper.unmount()}
 })
})
