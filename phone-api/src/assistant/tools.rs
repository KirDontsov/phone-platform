use super::store;
use crate::{
    auth as sessions, json, ok, settings, to_json, ApiError, AppState, Document, Uuid, Value,
};
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{DateTime, NaiveDate};
use diesel::prelude::*;

pub const NAMES: [&str; 8] = [
    "get_workspace_configuration",
    "connect_telphin",
    "test_telphin_connection",
    "sync_telphin",
    "upsert_taxonomy",
    "upsert_filter",
    "save_rubric",
    "set_callback_rules",
];
fn object(properties: Value, required: Vec<&str>) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn string(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
fn version() -> Value {
    json!({"type":"integer","minimum":1})
}
fn revision() -> Value {
    json!({"type":"integer","minimum":0})
}
pub fn catalog() -> Vec<Value> {
    let specs=vec![
 (NAMES[0],"Получить текущие настройки своей компании и revision перед изменением",object(json!({}),vec![])),
 (NAMES[1],"Подключить Телфин по защищённому credential_ref; секреты не читаются моделью",object(json!({"credential_ref":{"type":"string","format":"uuid"},"expected_revision":revision()}),vec!["credential_ref","expected_revision"])),
 (NAMES[2],"Проверить API Телфина своей компании; это не доказательство полноты истории",object(json!({}),vec![])),
 (NAMES[3],"Поставить импорт Телфина в очередь. Не объявлять полноту неподтверждённой истории",object(json!({"from":string(40),"to":string(40),"all_available":{"type":"boolean"},"expected_revision":revision()}),vec!["expected_revision"])),
 (NAMES[4],"Создать или обновить тег/бизнес-статус. Для изменения существующего нужны id/version и актуальная revision",object(json!({"kind":{"type":"string","enum":["tag","status"]},"name":string(80),"color":{"type":"string","pattern":"^#[0-9a-fA-F]{6}$"},"id":{"type":"string","format":"uuid"},"version":version(),"archived":{"type":"boolean"},"expected_revision":revision()}),vec!["kind","name","expected_revision"])),
 (NAMES[5],"Создать или изменить сохранённый фильтр с типизированным AST AND/OR/NOT",object(json!({"name":string(200),"ast":{"type":"object","description":"AST: leaf {op,field,value}; groups {op:and|or,rules:[AST]} or {op:not,rule:AST}. Max depth6/nodes64. Fields: direction,provider_result,employee,status_id,contact_kind,stage,callback_state,recording_state (strings); recording_state ready/pending/rate_limited/failed/none/unknown; ready means a downloaded recording can be played, has_recording only means the PBX reported one; score,duration (numbers); started_at (RFC3339 date); has_recording,human_connected (booleans); tag_ids (UUID array). Ops eq,ne,in,contains,gt,gte,lt,lte,is_null. For unresolved overdue missed calls use {op:eq,field:callback_state,value:overdue}; callback_minutes defines delay. contact_kind primary/repeat/unknown. Tag/status values use actual IDs from get_workspace_configuration. Example {op:and,rules:[{op:eq,field:callback_state,value:overdue},{op:contains,field:tag_ids,value:TAG_UUID}]}"},"id":{"type":"string","format":"uuid"},"version":version(),"expected_revision":revision()}),vec!["name","ast","expected_revision"])),
 (NAMES[6],"Создать immutable шкалу качества и при необходимости активировать. Старые анализы сохраняются",object(json!({"name":string(200),"criteria":{"type":"array","minItems":1,"maxItems":30,"items":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":100},"name":{"type":"string","minLength":1,"maxLength":200},"weight":{"type":"number","exclusiveMinimum":0,"maximum":1000},"required":{"type":"boolean"},"instruction":{"type":"string","maxLength":4000}},"required":["id","name","weight"],"additionalProperties":false}},"activate":{"type":"boolean"},"expected_revision":revision()}),vec!["name","criteria","expected_revision"])),
 (NAMES[7],"Настроить только часовой пояс, срок и правила перезвона; без удаления данных и управления доступом",object(json!({"timezone":string(80),"callback_minutes":{"type":"integer","minimum":1,"maximum":10080},"working_hours":{"type":["object","null"],"description":"Null or {enabled:false}, or {enabled:true,start:09:00,end:18:00,days:[1,2,3,4,5]}; days1=Mon..7=Sun, times HH:MM in timezone; supports overnight intervals.","properties":{"enabled":{"type":"boolean"},"start":{"type":"string"},"end":{"type":"string"},"days":{"type":"array","items":{"type":"integer","minimum":1,"maximum":7}}},"required":["enabled"],"additionalProperties":false},"callback_allow_inbound":{"type":"boolean"},"callback_min_duration":{"type":"integer","minimum":0,"maximum":3600},"expected_revision":revision()}),vec!["expected_revision"]))
 ];
    specs.into_iter().map(|(name,description,input)|json!({"name":name,"description":description,"inputSchema":input})).collect()
}
fn validate(name: &str, args: &Value) -> Result<(), ApiError> {
    let spec = catalog()
        .into_iter()
        .find(|v| v["name"] == name)
        .ok_or_else(|| ApiError::bad("Неизвестный инструмент помощника"))?;
    let schema = &spec["inputSchema"];
    let properties = schema["properties"].as_object().unwrap();
    let obj = args
        .as_object()
        .ok_or_else(|| ApiError::bad("Нужен JSON объект аргументов"))?;
    if args.to_string().len() > 65536 || obj.keys().any(|k| !properties.contains_key(k)) {
        return Err(ApiError::bad("Неподдерживаемые аргументы инструмента"));
    }
    for key in schema["required"].as_array().unwrap() {
        if !obj.contains_key(key.as_str().unwrap()) {
            return Err(ApiError::bad(
                "Не хватает обязательных аргументов инструмента",
            ));
        }
    }
    for (k, v) in obj {
        let def = &properties[k];
        let type_ok = match def["type"].as_str() {
            Some("string") => v.is_string(),
            Some("integer") => v.is_i64(),
            Some("boolean") => v.is_boolean(),
            Some("array") => v.is_array(),
            Some("object") => v.is_object(),
            _ => def["type"].is_array() && (v.is_object() || v.is_null()),
        };
        if !type_ok {
            return Err(ApiError::bad("Неверный тип аргумента инструмента"));
        }
        if let Some(text) = v.as_str() {
            let len = text.chars().count();
            if len < def["minLength"].as_u64().unwrap_or(0) as usize
                || len > def["maxLength"].as_u64().unwrap_or(4096) as usize
            {
                return Err(ApiError::bad("Неверная длина аргумента"));
            }
            if def["format"] == "uuid" && Uuid::parse_str(text).is_err() {
                return Err(ApiError::bad("Неверный ID"));
            }
        }
        if let Some(number) = v.as_i64() {
            if def["minimum"].as_i64().is_some_and(|v| number < v)
                || def["maximum"].as_i64().is_some_and(|v| number > v)
            {
                return Err(ApiError::bad("Число вне допустимых границ"));
            }
        }
        if let Some(options) = def["enum"].as_array() {
            if !options.contains(v) {
                return Err(ApiError::bad("Неверное значение аргумента"));
            }
        }
    }
    // Enforce the published nested schema at the authority boundary as well as SDK AJV.
    if name == "save_rubric" {
        if let Some(criteria) = args["criteria"].as_array() {
            for criterion in criteria {
                store::strict_keys(
                    criterion,
                    &["id", "name", "weight", "required", "instruction"],
                )?;
                if criterion["id"]
                    .as_str()
                    .is_some_and(|id| id.chars().count() > 100)
                {
                    return Err(ApiError::bad("Слишком длинный ID критерия"));
                }
                if criterion.get("instruction").is_some_and(|v| !v.is_string()) {
                    return Err(ApiError::bad("Инструкция критерия должна быть строкой"));
                }
            }
        }
    }
    if let Some(hours) = args.get("working_hours").filter(|v| !v.is_null()) {
        store::strict_keys(hours, &["enabled", "start", "end", "days"])?;
        if !hours["enabled"].is_boolean()
            || ["start", "end"]
                .iter()
                .any(|key| hours.get(*key).is_some_and(|v| !v.is_string()))
            || hours.get("days").is_some_and(|v| {
                v.as_array().is_none_or(|days| {
                    days.iter()
                        .any(|d| !d.as_i64().is_some_and(|v| (1..=7).contains(&v)))
                })
            })
        {
            return Err(ApiError::bad("Неверный формат рабочего времени"));
        }
    }
    if args.get("id").is_some() && args.get("version").is_none() {
        return Err(ApiError::new(
            409,
            "version_required",
            "Для изменения объекта передайте его текущую версию",
        ));
    }
    if name == "sync_telphin" {
        for field in ["from", "to"] {
            if let Some(v) = args[field].as_str() {
                if DateTime::parse_from_rfc3339(v).is_err()
                    && NaiveDate::parse_from_str(v, "%Y-%m-%d").is_err()
                {
                    return Err(ApiError::bad("Неверный формат даты импорта"));
                }
            }
        }
    }
    Ok(())
}
fn lookup(
    c: &mut diesel::PgConnection,
    ctx: &sessions::Context,
    kind: &str,
    args: &Value,
) -> Result<Option<Document>, ApiError> {
    if let Some(id) = args["id"].as_str() {
        let doc = settings::document(
            c,
            ctx.organization_id,
            Uuid::parse_str(id).map_err(|_| ApiError::bad("Неверный ID"))?,
            kind,
        )?;
        check_version(&doc, args)?;
        return Ok(Some(doc));
    }
    let name = args["name"].as_str().unwrap_or("").trim().to_lowercase();
    let found = persistence::list_documents(c, ctx.organization_id, kind)?
        .into_iter()
        .filter(|d| {
            d.data["name"]
                .as_str()
                .is_some_and(|v| v.trim().to_lowercase() == name)
        })
        .collect::<Vec<_>>();
    if found.len() > 1 {
        return Err(ApiError::new(
            409,
            "ambiguous_name",
            "Есть несколько объектов с таким именем. Укажите ID и версию",
        ));
    }
    let found = found.into_iter().next();
    if let Some(doc) = &found {
        check_version(doc, args)?;
    }
    Ok(found)
}
fn check_version(doc: &Document, args: &Value) -> Result<(), ApiError> {
    if args["version"].as_i64().is_some_and(|v| v != doc.version) {
        return Err(ApiError::new(
            409,
            "conflict",
            "Версия настройки изменилась",
        ));
    }
    Ok(())
}
fn snapshot(c: &mut diesel::PgConnection, ctx: &sessions::Context) -> Result<Value, ApiError> {
    let revision = settings::lock_settings_revision(c, ctx.organization_id)?;
    let all = |c: &mut diesel::PgConnection, kind: &str| -> Result<Value, ApiError> {
        let documents = persistence::list_documents(c, ctx.organization_id, kind)?;
        if documents.len() > 100 {
            return Err(ApiError::new(
                413,
                "assistant_context_too_large",
                "Слишком много настроек для одного контекста помощника. Используйте экран настроек",
            ));
        }
        Ok(json!(documents
            .into_iter()
            .map(to_json)
            .collect::<Vec<_>>()))
    };
    let org = crate::calls::policy(c, ctx.organization_id)?;
    let id = crate::operations::connection_id(ctx.organization_id, "telphin");
    let telphin=persistence::get_document(c,ctx.organization_id,id)?.map(|d|json!({"id":d.id,"version":d.version,"state":d.data["state"],"host":d.data["config"]["host"],"last_error":d.data["last_error"],"last_sync_at":d.data["last_sync_at"],"history_start":d.data["config"]["history_start"],"history_start_verified":d.data["config"]["history_start_verified"]})).unwrap_or(json!({"state":"disconnected","version":0}));
    let policy = json!({"timezone":org["timezone"],"callback_minutes":org["callback_minutes"],"working_hours":org["working_hours"],"callback_allow_inbound":org["callback_allow_inbound"],"callback_min_duration":org["callback_min_duration"]});
    Ok(
        json!({"revision":revision,"tags":all(c,"tag")?,"statuses":all(c,"status")?,"filters":all(c,"filter")?,"rubrics":all(c,"rubric")?,"callback_rules":policy,"telphin":telphin,"sync_runs":persistence::list_documents(c,ctx.organization_id,"sync_run")?.into_iter().take(5).map(|d|json!({"id":d.id,"state":d.data["state"],"imported":d.data["imported"],"from":d.data["from"],"to":d.data["to"]})).collect::<Vec<_>>()}),
    )
}
fn execute(
    c: &mut diesel::PgConnection,
    state: &AppState,
    ctx: &sessions::Context,
    run: &Document,
    name: &str,
    args: Value,
    receipt: Uuid,
) -> Result<Value, ApiError> {
    if name == NAMES[0] {
        return snapshot(c, ctx);
    }
    let revision = settings::lock_settings_revision(c, ctx.organization_id)?;
    let expected = args["expected_revision"].as_i64().ok_or_else(|| {
        ApiError::new(
            409,
            "revision_required",
            "Перед изменением получите текущую revision",
        )
    })?;
    if expected != revision {
        return Err(ApiError::new(
            409,
            "settings_changed",
            "Настройки изменились. Получите актуальные данные перед повтором",
        ));
    }
    let result = match name {
        "connect_telphin" => {
            let id = crate::id_field(&args, "credential_ref")?;
            let mut credential = store::document(c, ctx, id, store::CREDENTIAL)?;
            if credential.parent_id != run.parent_id || !store::valid_deadline(&credential.data) {
                return Err(ApiError::new(
                    400,
                    "credential_unavailable",
                    "Доступы истекли или относятся к другому чату",
                ));
            }
            let config = contracts::unseal(
                credential.data["config_encrypted"]
                    .as_str()
                    .ok_or_else(|| {
                        ApiError::new(
                            400,
                            "credential_unavailable",
                            "Доступы уже использованы; добавьте их заново",
                        )
                    })?,
                &state.key,
            )
            .map_err(|_| ApiError::internal())?;
            let result = settings::save_connection_in_db(
                c,
                ctx,
                "telphin".into(),
                config,
                state.key,
                state.public_url.clone(),
                state.demo,
                None,
            )?;
            credential
                .data
                .as_object_mut()
                .unwrap()
                .remove("config_encrypted");
            credential.data["consumed"] = json!(true);
            let data = credential.data.clone();
            store::save(c, credential, data)?;
            json!({"configured":true,"connection_id":result["id"],"provider":"telphin"})
        }
        "sync_telphin" => {
            let connection = crate::operations::connection_id(ctx.organization_id, "telphin");
            let doc = settings::document(c, ctx.organization_id, connection, "connection")?;
            settings::config_value(&doc, &state.key)?;
            let mut payload = args.clone();
            payload.as_object_mut().unwrap().remove("expected_revision");
            let job = persistence::enqueue(
                c,
                ctx.organization_id,
                "sync",
                Some(connection),
                &format!("assistant:{receipt}"),
                payload,
            )?;
            settings::audit(
                c,
                ctx,
                "sync.requested",
                Some(job.id),
                json!({"source":"assistant"}),
            )?;
            json!({"job_id":job.id,"queued":true,"history_completeness":"not_confirmed_by_this_action"})
        }
        "upsert_taxonomy" => {
            let kind = args["kind"].as_str().unwrap();
            if let Some(doc) = lookup(c, ctx, kind, &args)? {
                let unchanged = ["name", "color", "archived"]
                    .iter()
                    .all(|k| args.get(*k).is_none_or(|v| v == &doc.data[*k]));
                if unchanged {
                    to_json(doc)
                } else {
                    if args.get("version").is_none() {
                        return Err(ApiError::new(
                            409,
                            "version_required",
                            "Для изменения существующей метки нужны ID и версия",
                        ));
                    }
                    settings::patch_taxonomy_in_db(c, ctx, doc.id, args)?
                }
            } else {
                settings::create_taxonomy_in_db(c, ctx, Uuid::new_v5(&receipt, b"taxonomy"), args)?
            }
        }
        "upsert_filter" => {
            if let Some(doc) = lookup(c, ctx, "filter", &args)? {
                if doc.data["name"] == args["name"] && doc.data["ast"] == args["ast"] {
                    to_json(doc)
                } else {
                    let version = args["version"].as_i64().ok_or_else(|| {
                        ApiError::new(
                            409,
                            "version_required",
                            "Для изменения фильтра нужны ID и версия",
                        )
                    })?;
                    settings::save_filter_in_db(c, ctx, doc.id, args, Some(version))?
                }
            } else {
                settings::save_filter_in_db(c, ctx, Uuid::new_v5(&receipt, b"filter"), args, None)?
            }
        }
        "save_rubric" => {
            settings::validate_rubric(&args)?;
            let existing = persistence::list_documents(c, ctx.organization_id, "rubric")?
                .into_iter()
                .find(|d| d.data["name"] == args["name"] && d.data["criteria"] == args["criteria"]);
            let mut result = if let Some(d) = existing {
                to_json(d)
            } else {
                settings::create_rubric_in_db(
                    c,
                    ctx,
                    Uuid::new_v5(&receipt, b"rubric"),
                    args.clone(),
                )?
            };
            if args["activate"] == true && result["active"] != true {
                let id = crate::id_field(&result, "id")?;
                settings::activate_rubric_in_db(c, ctx, id)?;
                result = to_json(settings::document(c, ctx.organization_id, id, "rubric")?);
            }
            result
        }
        "set_callback_rules" => {
            if args.as_object().unwrap().len() < 2 {
                return Err(ApiError::bad("Укажите изменения правил перезвона"));
            }
            let mut b = args;
            b.as_object_mut().unwrap().remove("expected_revision");
            let result = settings::patch_org_in_db(c, ctx, b)?;
            json!({"timezone":result["timezone"],"callback_minutes":result["callback_minutes"],"working_hours":result["working_hours"],"callback_allow_inbound":result["callback_allow_inbound"],"callback_min_duration":result["callback_min_duration"]})
        }
        _ => return Err(ApiError::bad("Неизвестный инструмент")),
    };
    let current = settings::lock_settings_revision(c, ctx.organization_id)?;
    Ok(json!({"revision":current,"value":result}))
}
fn call_id(body: &Value) -> Result<String, ApiError> {
    Ok(crate::str_field(body, "call_id")?
        .chars()
        .take(129)
        .collect::<String>())
    .and_then(|s| {
        if s.len() > 128 {
            Err(ApiError::bad("Слишком длинный ID вызова"))
        } else {
            Ok(s)
        }
    })
}
fn append_tool(
    c: &mut diesel::PgConnection,
    mut run: Document,
    name: &str,
    status: &str,
) -> Result<(), ApiError> {
    store::event(&mut run.data, "tool", name, status, Some(name));
    let data = run.data.clone();
    store::save(c, run, data)?;
    Ok(())
}
pub async fn gateway(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let hash = store::bearer(&req)?;
    let body = body.into_inner();
    let method = crate::str_field(&body, "method")?.to_owned();
    if method == "list" {
        store::strict_keys(&body, &["method"])?;
        let data = state
            .db(move |c| {
                c.transaction::<_, ApiError, _>(|c| {
                    store::authorized_run(c, &hash)?;
                    Ok(json!({"tools":catalog()}))
                })
            })
            .await?;
        return Ok(ok(data));
    }
    store::strict_keys(&body, &["method", "call_id", "name", "arguments"])?;
    if method != "call" {
        return Err(ApiError::bad("Неизвестный метод"));
    }
    let id = call_id(&body)?;
    let name = crate::str_field(&body, "name")?.to_owned();
    let args = body["arguments"].clone();
    if name.len() > 100 {
        return Err(ApiError::bad("Слишком длинное имя инструмента"));
    }
    if name == "test_telphin_connection" {
        return test_telphin(state, hash, id, args).await;
    }
    let config = state.get_ref().clone();
    let result=state.db(move|c|c.transaction::<Result<Value,ApiError>,ApiError,_>(|c|{
  let(mut run,ctx)=store::authorized_run(c,&hash)?;let receipt=Uuid::new_v5(&run.id,format!("tool:{id}").as_bytes());let fingerprint=sessions::hash(&json!({"name":name,"arguments":args}).to_string());
  if let Some(prior)=persistence::get_document(c,ctx.organization_id,receipt)?{
   if prior.data["args_hash"]!=fingerprint{return Err(ApiError::new(409,"idempotency_conflict","ID вызова уже использован с другими аргументами"));}
   return Ok(if prior.data["state"]=="failed"{Err(stored_error(&prior.data))}else{Ok(prior.data["result"].clone())});
  }
  let count=run.data["tool_count"].as_u64().unwrap_or(0);if count>=24{return Err(ApiError::new(429,"assistant_tool_limit","Достигнут лимит действий в одном сообщении"));}
  run.data["tool_count"]=json!(count+1);let data=run.data.clone();run=store::save(c,run,data)?;
  let outcome=c.transaction::<_,ApiError,_>(|c|{validate(&name,&args)?;let result=execute(c,&config,&ctx,&run,&name,args,receipt)?;
   if result.to_string().len()>128*1024{return Err(ApiError::new(413,"assistant_result_too_large","Настроек слишком много для одного ответа помощника"));}Ok(result)});
  let safe_name=if NAMES.contains(&name.as_str()){name.as_str()}else{"unsupported_tool"};
  let mut record=json!({"user_id":ctx.user_id,"args_hash":fingerprint,"tool_name":safe_name,"state":if outcome.is_ok(){"completed"}else{"failed"}});
  match &outcome{Ok(result)=>record["result"]=result.clone(),Err(error)=>{record["error_status"]=json!(error.status);record["error_code"]=json!(error.code);record["error_message"]=json!(error.message);}}
  persistence::save_document(c,ctx.organization_id,receipt,store::RECEIPT,Some(run.id),record,None)?;
  append_tool(c,run,safe_name,if outcome.is_ok(){"completed"}else{"failed"})?;settings::audit(c,&ctx,if outcome.is_ok(){"assistant.tool_completed"}else{"assistant.tool_failed"},Some(receipt),json!({"tool":safe_name}))?;Ok(outcome)
 })).await??;
    Ok(ok(json!({"result":result})))
}
fn stored_error(data: &Value) -> ApiError {
    const CODES: &[&str] = &[
        "invalid_request",
        "not_found",
        "settings_changed",
        "conflict",
        "already_exists",
        "ambiguous_name",
        "version_required",
        "credential_unavailable",
        "service_unavailable",
        "assistant_result_too_large",
        "assistant_context_too_large",
    ];
    let code = CODES
        .iter()
        .copied()
        .find(|v| Some(*v) == data["error_code"].as_str())
        .unwrap_or("assistant_tool_failed");
    ApiError::new(
        data["error_status"].as_u64().unwrap_or(400) as u16,
        code,
        data["error_message"]
            .as_str()
            .unwrap_or("Действие помощника не выполнено"),
    )
}
async fn test_telphin(
    state: web::Data<AppState>,
    hash: String,
    id: String,
    args: Value,
) -> Result<HttpResponse, ApiError> {
    let h = hash.clone();
    let key = state.key;
    let prepared=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  let(mut run,ctx)=store::authorized_run(c,&h)?;let receipt=Uuid::new_v5(&run.id,format!("tool:{id}").as_bytes());
  let fingerprint=sessions::hash(&json!({"name":"test_telphin_connection","arguments":args}).to_string());if let Some(prior)=persistence::get_document(c,ctx.organization_id,receipt)?{
   if prior.data["args_hash"]!=fingerprint{return Err(ApiError::new(409,"idempotency_conflict","ID вызова уже использован"));}
   if prior.data["state"]=="completed"{return Ok((Value::Null,Some(Ok(prior.data["result"].clone())),receipt,0i64));}
   if prior.data["state"]=="failed"{return Ok((Value::Null,Some(Err(stored_error(&prior.data))),receipt,0i64));}
   return Err(ApiError::new(409,"assistant_action_pending","Проверка уже выполняется или была прервана"));
  }
  let count=run.data["tool_count"].as_u64().unwrap_or(0);if count>=24{return Err(ApiError::new(429,"assistant_tool_limit","Достигнут лимит действий"));}
  run.data["tool_count"]=json!(count+1);
  let configured=c.transaction::<_,ApiError,_>(|c|{validate("test_telphin_connection",&args)?;let connection=settings::document(c,ctx.organization_id,crate::operations::connection_id(ctx.organization_id,"telphin"),"connection")?;Ok((settings::config_value(&connection,&key)?,connection.version))});
  match configured{
   Ok((config,version))=>{persistence::save_document(c,ctx.organization_id,receipt,store::RECEIPT,Some(run.id),json!({"user_id":ctx.user_id,"args_hash":fingerprint,"tool_name":"test_telphin_connection","state":"pending"}),None)?;append_tool(c,run,"test_telphin_connection","running")?;Ok((config,None,receipt,version))},
   Err(error)=>{persistence::save_document(c,ctx.organization_id,receipt,store::RECEIPT,Some(run.id),json!({"user_id":ctx.user_id,"args_hash":fingerprint,"tool_name":"test_telphin_connection","state":"failed","error_code":error.code,"error_status":error.status,"error_message":error.message}),None)?;append_tool(c,run,"test_telphin_connection","failed")?;Ok((Value::Null,Some(Err(error)),receipt,0))}
  }
 })).await?;
    if let Some(result) = prepared.1 {
        return Ok(ok(json!({"result":result?})));
    }
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(25),
        phone_worker::test_connection("telphin", &prepared.0),
    )
    .await;
    let result = match outcome {
        Ok(Ok(_)) => {
            json!({"connected":true,"history_completeness":"not_confirmed_by_connection_test"})
        }
        Ok(Err(e)) => json!({"connected":false,"error_code":e.code}),
        Err(_) => json!({"connected":false,"error_code":"provider_timeout"}),
    };
    let done = result.clone();
    state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                let (run, ctx) = store::authorized_run(c, &hash)?;
                let current = settings::document(
                    c,
                    ctx.organization_id,
                    crate::operations::connection_id(ctx.organization_id, "telphin"),
                    "connection",
                )?;
                if current.version != prepared.3 {
                    return Err(ApiError::new(
                        409,
                        "connection_changed",
                        "Подключение изменилось во время проверки",
                    ));
                }
                let mut receipt = store::document(c, &ctx, prepared.2, store::RECEIPT)?;
                receipt.data["state"] = json!("completed");
                receipt.data["result"] = done.clone();
                let data = receipt.data.clone();
                store::save(c, receipt, data)?;
                append_tool(
                    c,
                    run,
                    "test_telphin_connection",
                    if done["connected"] == true {
                        "completed"
                    } else {
                        "failed"
                    },
                )?;
                Ok(())
            })
        })
        .await?;
    Ok(ok(json!({"result":result})))
}
