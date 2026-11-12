import {describe,it,expect} from 'vitest'
import {mount} from '@vue/test-utils'
import AudioPlayer from '../src/features/audio-player/AudioPlayer.vue'
const pending={id:'record-a',state:'pending',ready:false},ready={...pending,state:'ready',ready:true}
describe('recording availability',()=>{
 it('shows downloading without requesting media before the file is ready',()=>{
  const wrapper=mount(AudioPlayer,{props:{callId:'call-a',recordings:[pending]}})
  try{expect(wrapper.find('audio').exists()).toBe(false);expect(wrapper.text()).toContain('Плеер появится автоматически');expect(wrapper.text()).not.toContain('недоступна')}finally{wrapper.unmount()}
 })
 it('explains provider throttling while leaving the recording unavailable for playback',()=>{
  const wrapper=mount(AudioPlayer,{props:{callId:'call-a',recordings:[{...pending,last_error:'provider_rate_limited'}]}})
  try{expect(wrapper.find('audio').exists()).toBe(false);expect(wrapper.text()).toContain('Телфин временно ограничил загрузку записей');expect(wrapper.text()).toContain('автоматически повторит попытку')}finally{wrapper.unmount()}
 })
 it('automatically mounts the same recording after its pending state becomes ready',async()=>{
  const wrapper=mount(AudioPlayer,{props:{callId:'call-a',recordings:[pending]}})
  try{expect(wrapper.find('audio').exists()).toBe(false);await wrapper.setProps({recordings:[ready]});expect(wrapper.find('audio').attributes('src')).toBe('/api/calls/call-a/media/record-a');expect(wrapper.text()).not.toContain('Плеер появится автоматически')}finally{wrapper.unmount()}
 })
 it('chooses a downloaded recording and keeps the same media node across polling updates',async()=>{
  const second={id:'record-b',state:'ready',ready:true},wrapper=mount(AudioPlayer,{props:{callId:'call-a',recordings:[pending,second]}})
  try{expect(wrapper.find('audio').attributes('src')).toContain('/record-b');const node=wrapper.find('audio').element;await wrapper.setProps({recordings:[{...second},{...pending}]});expect(wrapper.find('audio').element).toBe(node);expect(wrapper.find('audio').attributes('src')).toContain('/record-b')}finally{wrapper.unmount()}
 })
 it('does not treat a failed download as a playable or still-loading recording',()=>{
  const wrapper=mount(AudioPlayer,{props:{callId:'call-a',recordings:[{...pending,state:'failed'}]}})
  try{expect(wrapper.find('audio').exists()).toBe(false);expect(wrapper.text()).toContain('Не удалось загрузить запись');expect(wrapper.text()).not.toContain('Плеер появится автоматически')}finally{wrapper.unmount()}
 })
 it('clears a previous playback error when switching to another call',async()=>{
  const wrapper=mount(AudioPlayer,{props:{callId:'call-a',recordings:[ready]}})
  try{await wrapper.find('audio').trigger('error');expect(wrapper.text()).toMatch(/Запись недоступна|Не удалось воспроизвести/);await wrapper.setProps({callId:'call-b',recordings:[{...ready,id:'record-b'}]});expect(wrapper.text()).not.toMatch(/Запись недоступна|Не удалось воспроизвести/);expect(wrapper.find('audio').attributes('src')).toBe('/api/calls/call-b/media/record-b')}finally{wrapper.unmount()}
 })
})
