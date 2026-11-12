export const sheetFields:Record<string,string>={call_id:'Ключ звонка',date:'Дата звонка',started_at:'Дата и время',phone:'Телефон',contact_name:'Клиент',employee:'Сотрудник',direction:'Направление',result:'Результат звонка',provider_result:'Технический результат',duration:'Длительность',score:'Оценка качества',summary:'Итоги разговора',customer_intent:'Запрос клиента',agreements:'Договорённости',next_steps:'Следующие шаги',criteria:'Проверка требований',status_id:'ID бизнес-статуса',tag_ids:'ID тегов',contact_kind:'Тип обращения',call_link:'Ссылка на звонок',delivery_marker:'Метка доставки'}
export const defaultSheetMapping={call_id:'A',date:'B',phone:'C',summary:'D',score:'E',agreements:'F',next_steps:'G',criteria:'H',call_link:'I'}
export function validateSheetMapping(mapping:Record<string,string>){
 if(mapping.call_id!=='A')throw Error('Ключ звонка должен оставаться в колонке A')
 const columns=new Set<string>()
 for(const[field,column]of Object.entries(mapping)){
  if(!Object.prototype.hasOwnProperty.call(sheetFields,field))throw Error('Неизвестное поле таблицы: '+field)
  if(!/^(?:[A-Z]|A[A-Z])$/.test(column))throw Error('Укажи колонку от A до AZ')
  if(columns.has(column))throw Error('Для разных полей нужны разные колонки')
  columns.add(column)
 }
 return{...mapping}
}
