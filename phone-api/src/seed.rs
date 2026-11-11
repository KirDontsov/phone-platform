//! Explicit synthetic demo seed. Never reads or writes provider accounts.
use crate::*;
use diesel::{
    prelude::*,
    sql_types::{Jsonb, Text, Uuid as SqlUuid},
};
use std::io::Write;

#[derive(QueryableByName)]
struct UserId {
    #[diesel(sql_type=SqlUuid)]
    id: Uuid,
}
#[derive(QueryableByName)]
struct Row {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
fn id(org: Uuid, name: &str) -> Uuid {
    Uuid::new_v5(&org, name.as_bytes())
}
pub fn demo_wav(path: &std::path::Path) -> Result<(), ApiError> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ApiError::internal())?;
    }
    let samples = 16000u32 * 32;
    let bytes = samples * 2;
    let mut f = std::fs::File::create(path).map_err(|_| ApiError::internal())?;
    f.write_all(b"RIFF")
        .and_then(|_| f.write_all(&(36 + bytes).to_le_bytes()))
        .and_then(|_| f.write_all(b"WAVEfmt "))
        .and_then(|_| f.write_all(&16u32.to_le_bytes()))
        .and_then(|_| f.write_all(&1u16.to_le_bytes()))
        .and_then(|_| f.write_all(&1u16.to_le_bytes()))
        .and_then(|_| f.write_all(&16000u32.to_le_bytes()))
        .and_then(|_| f.write_all(&32000u32.to_le_bytes()))
        .and_then(|_| f.write_all(&2u16.to_le_bytes()))
        .and_then(|_| f.write_all(&16u16.to_le_bytes()))
        .and_then(|_| f.write_all(b"data"))
        .and_then(|_| f.write_all(&bytes.to_le_bytes()))
        .map_err(|_| ApiError::internal())?;
    for n in 0..samples {
        let t = n as f64 / 16000.;
        let envelope = if (t as u32) % 4 < 2 { 0.08 } else { 0. };
        let value = (32767. * envelope * (t * 440. * std::f64::consts::TAU).sin()) as i16;
        f.write_all(&value.to_le_bytes())
            .map_err(|_| ApiError::internal())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| ApiError::internal())?;
    }
    Ok(())
}
pub async fn seed_demo(state: &AppState) -> Result<(), ApiError> {
    if !state.demo {
        return Ok(());
    }
    let email = std::env::var("BOOTSTRAP_EMAIL").unwrap_or_else(|_| "owner@demo.local".into());
    let password = std::env::var("BOOTSTRAP_PASSWORD")
        .map_err(|_| ApiError::bad("BOOTSTRAP_PASSWORD обязателен для DEMO_MODE"))?;
    let name = std::env::var("BOOTSTRAP_NAME").unwrap_or_else(|_| "Анна Смирнова".into());
    let org_name =
        std::env::var("BOOTSTRAP_ORGANIZATION").unwrap_or_else(|_| "Атлас · Продажи".into());
    let key = state.key;
    let media = state.media_root.clone();
    state.db(move|c|{
  for (org_name,count) in [(org_name,48usize),("Орбита · Сервис".into(),12usize)]{
   let(org,_)=auth::bootstrap(c,&email,&password,&name,&org_name)?;
   let settings=diesel::sql_query("SELECT data FROM organizations WHERE id=$1").bind::<SqlUuid,_>(org).get_result::<Row>(c)?.data;
   if settings["demo_seeded"]==true{
    demo_wav(&media.join(format!("{org}/demo-signal.wav")))?;
    diesel::sql_query("UPDATE documents SET data=data||'{\"synthetic\":true}'::jsonb WHERE organization_id=$1 AND kind='call' AND data->>'demo'='true' AND NOT(data?'synthetic')").bind::<SqlUuid,_>(org).execute(c)?;
    diesel::sql_query("UPDATE documents SET data=data||'{\"duration\":32.0}'::jsonb WHERE organization_id=$1 AND kind='recording' AND data->>'demo'='true' AND NOT(data?'duration')").bind::<SqlUuid,_>(org).execute(c)?;
    continue;
   }
   c.transaction::<_,ApiError,_>(|c|{
    for (mail,full,role,extension) in [("manager@demo.local","Дмитрий Волков","manager","101"),("reviewer@demo.local","Елена Орлова","reviewer","102")]{
     let hash=auth::password_hash(&password)?;let user=diesel::sql_query("INSERT INTO users(email,name,password_hash) VALUES($1,$2,$3) ON CONFLICT(email) DO UPDATE SET email=EXCLUDED.email RETURNING id").bind::<Text,_>(mail).bind::<Text,_>(full).bind::<Text,_>(hash).get_result::<UserId>(c)?.id;
     diesel::sql_query("INSERT INTO memberships(organization_id,user_id,role,extension) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(user).bind::<Text,_>(role).bind::<Text,_>(extension).execute(c)?;
    }
    let tag_defs=[("consultation","Консультация","#0d9488"),("price","Стоимость","#6366f1"),("objection","Возражение","#f59e0b"),("meeting","Встреча","#2563eb"),("review","Нужна проверка","#ef4444")];
    let statuses=[("new","Новое обращение","#6366f1"),("callback","Нужен перезвон","#f59e0b"),("resolved","Вопрос решён","#0d9488"),("meeting","Встреча назначена","#2563eb")];
    for(kind,defs)in [("tag",&tag_defs[..]),("status",&statuses[..])]{for(key,name,color)in defs{persistence::save_document(c,org,id(org,&format!("{kind}:{key}")),kind,None,json!({"name":name,"color":color,"archived":false}),None)?;}}
    let criteria=json!([
     {"id":"greeting","name":"Приветствие и представление","weight":1,"required":true,"instruction":"Менеджер представился и обозначил компанию"},
     {"id":"needs","name":"Выявление потребности","weight":2,"required":true,"instruction":"Менеджер уточнил задачу клиента и важные ограничения"},
     {"id":"solution","name":"Полнота ответа","weight":2,"required":true,"instruction":"Ответ понятен и соответствует запросу клиента"},
     {"id":"next_step","name":"Следующий шаг","weight":1,"required":false,"instruction":"Согласованы конкретные дальнейшие действия"}
    ]);
    let rubric=id(org,"rubric:default");persistence::save_document(c,org,rubric,"rubric",None,json!({"name":"Качество диалога","rubric_version":1,"active":true,"criteria":criteria}),None)?;
    for kind in ["asr","analysis"]{let config=json!({"provider":"demo","model":"synthetic-demo-v1","budget_daily":100,"timeout_seconds":60});let encrypted=contracts::seal(&config,&key).map_err(|_|ApiError::internal())?;
     persistence::save_document(c,org,crate::operations::connection_id(org,kind),"connection",None,json!({"kind":kind,"state":"configured","config":config,"config_encrypted":encrypted,"last_error":null,"last_sync_at":null}),None)?;
    }
    for kind in ["telphin","amocrm","sheets"]{persistence::save_document(c,org,crate::operations::connection_id(org,kind),"connection",None,json!({"kind":kind,"state":"disconnected","config":{},"last_error":null,"last_sync_at":null}),None)?;}
    let relative=format!("{org}/demo-signal.wav");demo_wav(&media.join(&relative))?;
    let names=["Мария Лебедева","Алексей Миронов","Светлана Крылова","Игорь Соколов","Татьяна Власова","Андрей Новиков","Ольга Кузнецова","Павел Романов","Юлия Морозова","Михаил Орлов","Виктория Белова","Сергей Попов"];
    let employees=["Дмитрий Волков","Елена Орлова","Анна Смирнова","Максим Серов"];
    let now=Utc::now();
    for index in 0..count{
     let call_id=id(org,&format!("demo-call:{index}"));let time=now-chrono::Duration::minutes((index as i64)*187+23);
     let result=match index%8{0=>"not answered",1|2|6=>"bridged",3=>"answered",4=>"busy",5=>"rejected",_=>"failed"};let connected=result=="bridged";let direction=if [2,4,7].contains(&(index%8)){"out"}else{"in"};
     let processed=connected&&index%5!=0;let score=if processed{Some((64+index%34)as f64)}else{None};let tag_ids=if connected{vec![id(org,"tag:consultation"),id(org,if index%2==0{"tag:meeting"}else{"tag:price"})]}else{vec![]};
     let status=id(org,if connected{if index%2==0{"status:meeting"}else{"status:resolved"}}else{"status:callback"});
     let call=json!({"provider_id":format!("DEMO-{index:05}"),"started_at":time,"phone":format!("+7000000{:04}",index%12),"contact_name":names[index%12],"employee":employees[index%4],"direction":direction,"provider_result":result,"human_connected":connected,"duration":if connected{152+index*13}else{18+index%30},"stage":if processed{"ready"}else if connected{"imported"}else{"skipped"},"status_id":status,"tag_ids":tag_ids,"contact_kind":"unknown","score":score,"has_recording":connected,"note":"","manual_lock":false,"coverage_complete":true,"demo":true,"synthetic":true,"demo_scenario":index%4});
     persistence::save_document(c,org,call_id,"call",None,call,None)?;
     persistence::save_document(c,org,id(call_id,"leg:0"),"leg",Some(call_id),json!({"flow":direction,"extension_id":101+index%4,"extension_name":employees[index%4],"result":result,"started_at":time,"duration":if connected{152+index*13}else{18}}),None)?;
     if !connected{continue;}
     let recording=id(call_id,"recording:demo");persistence::save_document(c,org,recording,"recording",Some(call_id),json!({"call_id":call_id,"record_uuid":format!("DEMO-AUDIO-{index}"),"path":relative,"mime":"audio/wav","size":1024044,"duration":32.0,"state":"ready","ready":true,"demo":true,"description":"Синтетический аудиосигнал для проверки плеера"}),None)?;
     let segments=json!([
       {"id":"s1","speaker":"Менеджер","role":"manager","start":0.0,"end":5.0,"text":"Добрый день! Компания Атлас, меня зовут Дмитрий. Чем могу помочь?"},
       {"id":"s2","speaker":"Клиент","role":"client","start":5.0,"end":10.0,"text":"Здравствуйте. Хотелось бы узнать стоимость и сроки подключения для нашей команды."},
       {"id":"s3","speaker":"Менеджер","role":"manager","start":10.0,"end":15.0,"text":"Подскажите, сколько сотрудников будут пользоваться сервисом и когда планируете начать работу?"},
       {"id":"s4","speaker":"Клиент","role":"client","start":15.0,"end":20.0,"text":"У нас двенадцать сотрудников. Хотим запуститься в следующем месяце, важно сохранить историю обращений."},
       {"id":"s5","speaker":"Менеджер","role":"manager","start":20.0,"end":26.0,"text":"История сохранится. Я подготовлю предложение для двенадцати сотрудников и отправлю условия на вашу почту."},
       {"id":"s6","speaker":"Менеджер","role":"manager","start":26.0,"end":32.0,"text":"Давайте договоримся о демонстрации завтра в одиннадцать. После встречи обсудим оставшиеся вопросы."}
     ]);
     let transcript_id=id(call_id,"transcript:demo");persistence::save_document(c,org,transcript_id,"transcript",Some(call_id),json!({"call_id":call_id,"transcript_version":1,"provider":"demo","state":"ready","language":"ru","role_confidence":"known","segments":segments,"created_at":time+chrono::Duration::minutes(3),"demo":true}),None)?;
     if let Some(score)=score{
      let criteria=criteria.as_array().unwrap().iter().enumerate().map(|(n,r)|{let segment=&segments[match n{0=>0,1=>2,2=>4,_=>5}];json!({"id":r["id"],"name":r["name"],"weight":r["weight"],"score":score/20.,"status":"scored","evidence":[{"segment_id":segment["id"],"start":segment["start"],"end":segment["end"],"quote":segment["text"]}],"reason":"Демонстрационная оценка синтетического диалога"})}).collect::<Vec<_>>();
      persistence::save_document(c,org,id(call_id,"analysis:demo"),"analysis",Some(call_id),json!({"call_id":call_id,"state":"ready","model":"synthetic-demo-v1","provider":"demo","prompt_version":1,"rubric_version":1,"transcript_id":transcript_id,"summary":"Клиент уточнил стоимость и сроки подключения для команды из 12 сотрудников. Менеджер выяснил потребность, подтвердил сохранение истории и предложил демонстрацию.","customer_intent":"Подключить команду к сервису в следующем месяце","agreements":["Подготовить предложение для 12 сотрудников","Отправить условия на почту"],"next_steps":["Провести демонстрацию завтра в 11:00"],"suggested_status_id":status,"suggested_tag_ids":tag_ids,"criteria":criteria,"score":score,"abstention_reason":null,"usage":{"input_tokens":0,"output_tokens":0,"cost":0.0},"created_at":time+chrono::Duration::minutes(4),"demo":true}),None)?;
     }
    }
    persistence::save_document(c,org,id(org,"filter:overdue"),"filter",None,json!({"name":"Просроченные перезвоны","ast":{"field":"callback_state","op":"eq","value":"overdue"}}),None)?;
    persistence::save_document(c,org,id(org,"filter:low-score"),"filter",None,json!({"name":"Нужна проверка качества","ast":{"op":"and","rules":[{"field":"score","op":"lt","value":75},{"field":"score","op":"is_null","value":false}]}}),None)?;
    persistence::save_document(c,org,Uuid::new_v4(),"audit",None,json!({"action":"demo.created","user_name":"Система","details":{"synthetic":true,"calls":count},"created_at":now}),None)?;
    diesel::sql_query("UPDATE organizations SET data=data||'{\"demo_seeded\":true}'::jsonb WHERE id=$1").bind::<SqlUuid,_>(org).execute(c)?;Ok(())
   })?;
  }Ok(())
 }).await
}
