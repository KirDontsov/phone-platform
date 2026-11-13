import type {Call,RecordingState} from '../interfaces'
export const recordingLabels:Record<RecordingState,string>={ready:'Можно слушать',pending:'Загружается',rate_limited:'Ожидает Телфин',failed:'Ошибка загрузки',none:'Нет записи',unknown:'Не загружена'}
export const recordingPresentation:Record<RecordingState,{icon:string;color:string;description:string}>={
 ready:{icon:'play',color:'teal',description:'Запись доступна для прослушивания в карточке звонка'},
 pending:{icon:'clock',color:'outline',description:'Запись ещё загружается или ожидает загрузки'},
 rate_limited:{icon:'clock',color:'amber',description:'Телфин временно ограничил запросы. Запись ещё не скачана; загрузка продолжится автоматически после паузы. Уже скачанные записи можно слушать'},
 failed:{icon:'alert',color:'red',description:'Не удалось загрузить запись'},
 none:{icon:'close',color:'outline',description:'У этого звонка нет записи'},
 unknown:{icon:'download',color:'outline',description:'Запись ещё не загружена; доступность для прослушивания не подтверждена'},
}
export function recordingState(call:Pick<Call,'has_recording'|'recording_state'>):RecordingState {if(call.recording_state&&Object.prototype.hasOwnProperty.call(recordingLabels,call.recording_state))return call.recording_state;return call.has_recording===false?'none':'unknown'}
