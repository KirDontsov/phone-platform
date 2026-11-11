export const resultLabels:Record<string,string>={'bridged':'Разговор состоялся','answered':'Ответ АТС / IVR','not answered':'Пропущен','rejected':'Отклонён','busy':'Занято','failed':'Ошибка','unknown':'Неизвестно'}
export const stageLabels:Record<string,string>={imported:'Загружен',recording_pending:'Ожидает запись',transcribing:'Расшифровывается',analyzing:'Анализируется',ready:'Готов',partial:'Частично',failed:'Ошибка',skipped:'Без анализа'}
export const kindLabels:Record<string,string>={primary:'Первичное',repeat:'Повторное',unknown:'Не определено'}
export const callbackLabels:Record<string,string>={open:'Ожидает связи',overdue:'Просрочено',resolved:'Обработано',uncertain:'Недостаточно истории'}
export function duration(value:number){if(!Number.isFinite(value))return'—';return `${Math.floor(value/60)}:${String(Math.round(value%60)).padStart(2,'0')}`}
export function datetime(value:unknown,timezone='Europe/Moscow'){if(!value)return'—';const date=new Date(String(value));return Number.isNaN(date.valueOf())?'—':new Intl.DateTimeFormat('ru-RU',{day:'2-digit',month:'short',hour:'2-digit',minute:'2-digit',timeZone:timezone}).format(date)}
export function shortDate(value:string){const d=new Date(value);return Number.isNaN(d.valueOf())?value:new Intl.DateTimeFormat('ru-RU',{day:'numeric',month:'short'}).format(d)}
export const percent=(value:number,total:number)=>total?Math.min(100,Math.round(value/total*100)):0
export function text(value:unknown){return value===null||value===undefined?'—':typeof value==='object'?JSON.stringify(value):String(value)}

export function providerCost(value:unknown){return typeof value==='number'&&Number.isFinite(value)?new Intl.NumberFormat('ru-RU',{maximumFractionDigits:8}).format(value):'неизвестна'}
