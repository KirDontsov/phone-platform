use crate::*;
use actix_web::HttpRequest;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{NaiveDate, TimeZone};
use diesel::{
    prelude::*,
    sql_types::{Jsonb, Uuid as SqlUuid},
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

#[derive(QueryableByName)]
struct Policy {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
pub fn policy(conn: &mut diesel::PgConnection, org: Uuid) -> Result<Value, ApiError> {
    Ok(diesel::sql_query("SELECT jsonb_build_object('timezone',timezone,'callback_minutes',callback_minutes,'retention_days',retention_days)||data AS data FROM organizations WHERE id=$1").bind::<SqlUuid,_>(org).get_result::<Policy>(conn)?.data)
}
#[derive(Default)]
struct RecordingAvailability {
    ready: bool,
    pending: bool,
    rate_limited: bool,
    failed: bool,
}
impl RecordingAvailability {
    fn include(&mut self, data: &Value) {
        if data["ready"] == true || data["state"] == "ready" {
            self.ready = true;
        } else if data["state"] == "failed" {
            self.failed = true;
        } else {
            self.pending = true;
            self.rate_limited |= data["last_error"] == "provider_rate_limited";
        }
    }
    fn state(&self) -> &'static str {
        if self.ready {
            "ready"
        } else if self.rate_limited {
            "rate_limited"
        } else if self.pending {
            "pending"
        } else if self.failed {
            "failed"
        } else {
            "unknown"
        }
    }
}
pub fn all_calls(conn: &mut diesel::PgConnection, org: Uuid) -> Result<Vec<Value>, ApiError> {
    let mut calls = persistence::list_documents(conn, org, "call")?
        .into_iter()
        .filter(|d| d.data["deleted"].as_bool() != Some(true))
        .map(to_json)
        .collect::<Vec<_>>();
    // One organization-scoped read; provider presence never implies a playable local file.
    let mut recordings: HashMap<Uuid, RecordingAvailability> = HashMap::new();
    for recording in persistence::list_documents(conn, org, "recording")? {
        if let Some(parent) = recording.parent_id {
            recordings
                .entry(parent)
                .or_default()
                .include(&recording.data);
        }
    }
    for call in &mut calls {
        let summary = call["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .and_then(|id| recordings.get(&id));
        call["recording_state"] =
            json!(summary
                .map(RecordingAvailability::state)
                .unwrap_or_else(|| if call["has_recording"] == false {
                    "none"
                } else {
                    "unknown"
                }));
    }
    let mut ordered = (0..calls.len()).collect::<Vec<_>>();
    ordered.sort_by_key(|&i| at(&calls[i]));
    let mut earlier = std::collections::HashSet::new();
    let mut position = 0;
    while position < ordered.len() {
        let time = at(&calls[ordered[position]]);
        let mut end = position + 1;
        while end < ordered.len() && at(&calls[ordered[end]]) == time {
            end += 1;
        }
        for &index in &ordered[position..end] {
            if calls[index]["manual_lock"].as_bool() != Some(true) {
                let kind = match phone_key(&calls[index]) {
                    Some(key) if earlier.contains(&key) => "repeat",
                    Some(_) if calls[index]["coverage_complete"] == true => "primary",
                    _ => "unknown",
                };
                calls[index]["contact_kind"] = json!(kind);
            }
        }
        for &index in &ordered[position..end] {
            if let Some(key) = phone_key(&calls[index]) {
                earlier.insert(key);
            }
        }
        position = end;
    }
    let now = Utc::now();
    let episodes = domain::callback_episodes(&calls, &policy(conn, org)?, now);
    let states = episodes
        .iter()
        .filter_map(|e| e["call_id"].as_str().map(|id| (id.to_owned(), e)))
        .collect::<HashMap<_, _>>();
    for call in &mut calls {
        let episode = states.get(call["id"].as_str().unwrap_or(""));
        call["callback_state"] = episode.map(|e| e["state"].clone()).unwrap_or(json!("none"));
        call["callback_attempts"] = episode
            .map(|e| e["attempts"].clone())
            .unwrap_or(Value::Null);
        call["callback_age_minutes"] = episode
            .and_then(|e| e["opened_at"].as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|opened| {
                json!((now - opened.with_timezone(&Utc)).num_seconds().max(0) as f64 / 60.)
            })
            .unwrap_or(Value::Null);
    }
    processing::Snapshot::load(conn, org)?.enrich(&mut calls);
    Ok(calls)
}
fn at(v: &Value) -> chrono::DateTime<Utc> {
    v["started_at"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(chrono::DateTime::<Utc>::UNIX_EPOCH)
}
fn coverage(calls: &[Value]) -> Value {
    let complete = !calls.is_empty()
        && calls
            .iter()
            .all(|c| c["coverage_complete"].as_bool() == Some(true));
    json!({"complete":complete,"reason":if complete{""}else if calls.is_empty(){"История пока не загружена"}else{"История загружена частично. Первичность и отсутствие перезвона могут требовать проверки"}})
}
#[derive(Deserialize, Default)]
pub struct Query {
    limit: Option<usize>,
    cursor: Option<String>,
    search: Option<String>,
    from: Option<String>,
    to: Option<String>,
    filter: Option<String>,
}
fn boundary(value: &str, tz: chrono_tz::Tz, end: bool) -> Result<chrono::DateTime<Utc>, ApiError> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let date = if end {
            date.succ_opt()
                .ok_or_else(|| ApiError::bad("Неверная дата"))?
        } else {
            date
        };
        return tz
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|d| d.with_timezone(&Utc))
            .ok_or_else(|| ApiError::bad("Неоднозначная дата"));
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| ApiError::bad("Неверный формат даты"))
}
fn filtered(mut calls: Vec<Value>, q: &Query, policy: &Value) -> Result<Vec<Value>, ApiError> {
    let timezone = policy["timezone"]
        .as_str()
        .unwrap_or("Europe/Moscow")
        .parse::<chrono_tz::Tz>()
        .unwrap_or(chrono_tz::Europe::Moscow);
    let from = q
        .from
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| boundary(s, timezone, false))
        .transpose()?;
    let to =
        q.to.as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| boundary(s, timezone, true))
            .transpose()?;
    if from.zip(to).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::bad("Начало периода должно быть раньше окончания"));
    }
    let ast = if let Some(raw) = q.filter.as_deref().filter(|s| !s.is_empty()) {
        if raw.len() > 32000 {
            return Err(ApiError::bad("Слишком большой фильтр"));
        }
        let ast: Value =
            serde_json::from_str(raw).map_err(|_| ApiError::bad("Неверный JSON фильтра"))?;
        domain::validate_filter(&ast).map_err(|_| ApiError::bad("Неверная структура фильтра"))?;
        Some(ast)
    } else {
        None
    };
    let search = q.search.as_deref().unwrap_or("").trim().to_lowercase();
    if search.len() > 300 {
        return Err(ApiError::bad("Слишком длинный поисковый запрос"));
    }
    calls.retain(|c| {
        let time = at(c);
        from.is_none_or(|v| time >= v)
            && to.is_none_or(|v| time < v)
            && (search.is_empty()
                || ["phone", "contact_name", "employee", "note"]
                    .iter()
                    .any(|k| {
                        c[*k]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&search)
                    }))
            && ast.as_ref().is_none_or(|a| domain::matches_filter(c, a))
    });
    calls.sort_by(|a, b| {
        at(b)
            .cmp(&at(a))
            .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
    });
    Ok(calls)
}
async fn list(
    state: web::Data<AppState>,
    req: HttpRequest,
    q: web::Query<Query>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    let q = q.into_inner();
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    Ok(ok(state
        .db(move |c| {
            let all = all_calls(c, ctx.organization_id)?;
            let cov = coverage(&all);
            let mut items = filtered(all, &q, &policy(c, ctx.organization_id)?)?;
            let total = items.len();
            let scope = auth::hash(&format!(
                "{}:{:?}:{:?}:{:?}:{:?}",
                ctx.organization_id, q.search, q.from, q.to, q.filter
            ));
            if let Some(cursor) = q.cursor.as_ref().filter(|s| !s.is_empty()) {
                let decoded = URL_SAFE_NO_PAD
                    .decode(cursor)
                    .map_err(|_| ApiError::bad("Неверный курсор"))?;
                let cursor: Value = serde_json::from_slice(&decoded)
                    .map_err(|_| ApiError::bad("Неверный курсор"))?;
                if cursor["scope"].as_str() != Some(&scope) {
                    return Err(ApiError::bad("Курсор относится к другому фильтру"));
                }
                let idx = items
                    .iter()
                    .position(|i| i["id"] == cursor["id"])
                    .ok_or_else(|| {
                        ApiError::new(
                            409,
                            "cursor_changed",
                            "Список изменился. Начните с первой страницы",
                        )
                    })?;
                items.drain(..=idx);
            }
            let more = items.len() > limit;
            items.truncate(limit);
            let next = if more {
                items.last().map(|i| {
                    URL_SAFE_NO_PAD.encode(json!({"scope":scope,"id":i["id"]}).to_string())
                })
            } else {
                None
            };
            Ok(json!({"items":items,"total":total,"next_cursor":next,"coverage":cov}))
        })
        .await?))
}
fn phone_key(v: &Value) -> Option<String> {
    let digits = v["phone"]
        .as_str()?
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    (digits.len() >= 7).then_some(digits)
}
async fn detail(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    let id = *id;
    let key = state.key;
    let demo = state.demo;
    Ok(ok(state.db(move|c|{
 settings::document(c,ctx.organization_id,id,"call")?;let all=all_calls(c,ctx.organization_id)?;let call=all.iter().find(|v|v["id"].as_str()==Some(&id.to_string())).cloned().ok_or_else(ApiError::missing)?;let children=persistence::list_children(c,ctx.organization_id,id,None)?;
 let child=|kind:&str|children.iter().filter(|d|d.kind==kind).cloned().map(to_json).map(|mut v|{if kind=="leg"||kind=="recording"{if let Some(map)=v.as_object_mut(){for key in ["storage_url","record_url","path","filename","download_url"]{map.remove(key);}}}v}).rev().collect::<Vec<_>>();
 let history=if let Some(key)=phone_key(&call){let mut calls=all.into_iter().filter(|v|phone_key(v).as_ref()==Some(&key)&&v["id"]!=call["id"]).collect::<Vec<_>>();calls.sort_by_key(|v|std::cmp::Reverse(at(v)));calls.truncate(30);calls}else{vec![]};
 let mut audit=persistence::list_documents(c,ctx.organization_id,"audit")?.into_iter().filter(|d|d.data["subject_id"].as_str()==Some(&id.to_string())).map(to_json).take(50).collect::<Vec<_>>();audit.sort_by_key(|v|std::cmp::Reverse(v["created_at"].as_str().unwrap_or("").to_owned()));
 let snapshot=processing::Snapshot::load(c,ctx.organization_id)?;let mut processing=call["processing"].clone();processing["queue"]=snapshot.queue(&call);processing["providers"]=processing::providers(c,ctx.organization_id,&key,demo,&call)?;
 Ok(json!({"call":call,"processing":processing,"legs":child("leg"),"recordings":child("recording"),"transcripts":child("transcript"),"analyses":child("analysis"),"external_links":child("external_link"),"history":history,"audit":audit}))
 }).await?))
}
async fn annotate(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let id = *id;
    let b = body.into_inner();
    Ok(ok(state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
 let mut d=settings::document(c,ctx.organization_id,id,"call")?;let expected=b["version"].as_i64().unwrap_or(d.version);let old=d.data.clone();
 if let Some(value)=b.get("status_id"){if !value.is_null(){let id=Uuid::parse_str(value.as_str().ok_or_else(||ApiError::bad("Неверный статус"))?).map_err(|_|ApiError::bad("Неверный статус"))?;let status=settings::document(c,ctx.organization_id,id,"status")?;if status.data["archived"]==true&&old["status_id"]!=*value{return Err(ApiError::bad("Статус архивирован"));}}d.data["status_id"]=value.clone();}
 if let Some(value)=b.get("tag_ids"){let tags=value.as_array().filter(|t|t.len()<=30).ok_or_else(||ApiError::bad("Не более 30 тегов"))?;let mut seen=std::collections::HashSet::new();for tag in tags{let id=Uuid::parse_str(tag.as_str().ok_or_else(||ApiError::bad("Неверный тег"))?).map_err(|_|ApiError::bad("Неверный тег"))?;if !seen.insert(id){return Err(ApiError::bad("Повтор тега"));}let t=settings::document(c,ctx.organization_id,id,"tag")?;if t.data["archived"]==true&&!old["tag_ids"].as_array().is_some_and(|tags|tags.contains(tag)){return Err(ApiError::bad("Тег архивирован"));}}d.data["tag_ids"]=value.clone();}
 if let Some(note)=b.get("note"){if note.as_str().filter(|n|n.len()<=10000).is_none(){return Err(ApiError::bad("Слишком длинная заметка"));}d.data["note"]=note.clone();}
 if let Some(kind)=b.get("contact_kind"){if !["primary","repeat","unknown"].contains(&kind.as_str().unwrap_or("")){return Err(ApiError::bad("Неверный тип обращения"));}d.data["contact_kind"]=kind.clone();}
 if b.get("contact_kind").is_none()&&old["manual_lock"]!=true{if let Some(derived)=all_calls(c,ctx.organization_id)?.into_iter().find(|call|call["id"].as_str()==Some(&id.to_string())){d.data["contact_kind"]=derived["contact_kind"].clone();}}
 d.data["manual_lock"]=json!(b["manual_lock"].as_bool().unwrap_or(true));d.data["annotation_source"]=json!("manual");d.data["annotation_user_id"]=json!(ctx.user_id);
 let updated=persistence::save_document(c,ctx.organization_id,id,"call",d.parent_id,d.data,Some(expected))?;
 settings::audit(c,&ctx,"call.annotated",Some(id),json!({"manual_lock":updated.data["manual_lock"],"previous_status_id":old["status_id"],"status_id":updated.data["status_id"],"previous_tag_ids":old["tag_ids"],"tag_ids":updated.data["tag_ids"]}))?;Ok(to_json(updated))
 })).await?))
}
async fn reprocess(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let id = *id;
    let stage = body["stage"].as_str().unwrap_or("all").to_owned();
    if !["all", "transcribe", "analyze"].contains(&stage.as_str()) {
        return Err(ApiError::bad("Неизвестная стадия"));
    }
    let key = state.key;
    let demo = state.demo;
    let result = state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                diesel::sql_query(
                    "SELECT id FROM documents WHERE organization_id=$1 AND id=$2 FOR UPDATE",
                )
                .bind::<SqlUuid, _>(ctx.organization_id)
                .bind::<SqlUuid, _>(id)
                .execute(c)?;
                let d = settings::document(c, ctx.organization_id, id, "call")?;
                if let Some(job) = persistence::processing_jobs(c, ctx.organization_id)?
                    .into_iter()
                    .filter(|j| {
                        j.subject_id == Some(id)
                            && matches!(j.kind.as_str(), "reprocess" | "transcribe" | "analyze")
                            && processing::active(j)
                    })
                    .max_by_key(|j| (j.created_at, j.id))
                {
                    return Ok(json!({"job_id":job.id,"reused":true}));
                }
                processing::preflight(c, ctx.organization_id, &key, demo, &d, &stage)?;
                let job = persistence::enqueue(
                    c,
                    ctx.organization_id,
                    "reprocess",
                    Some(id),
                    &format!("reprocess:{id}:{}", Uuid::new_v4()),
                    json!({"stage":stage,"input_version":d.version}),
                )?;
                settings::audit(
                    c,
                    &ctx,
                    "call.reprocess",
                    Some(id),
                    json!({"job_id":job.id,"stage":stage}),
                )?;
                Ok(json!({"job_id":job.id}))
            })
        })
        .await?;
    if result["reused"] == true {
        Ok(HttpResponse::Accepted()
            .insert_header(("Cache-Control", "no-store"))
            .json(json!({"data":result})))
    } else {
        Ok(ok(result))
    }
}
async fn delete(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let id = *id;
    state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                let mut d = settings::document(c, ctx.organization_id, id, "call")?;
                d.data["deleted"] = json!(true);
                persistence::save_document(
                    c,
                    ctx.organization_id,
                    id,
                    "call",
                    d.parent_id,
                    d.data.clone(),
                    Some(d.version),
                )?;
                persistence::tombstone(c, ctx.organization_id, id, d.data["provider_id"].as_str())?;
                persistence::enqueue(
                    c,
                    ctx.organization_id,
                    "purge",
                    Some(id),
                    &format!("purge:{id}"),
                    json!({}),
                )?;
                settings::audit(c, &ctx, "call.deleted", Some(id), json!({}))?;
                Ok(())
            })
        })
        .await?;
    Ok(ok(json!({"ok":true})))
}
async fn media(
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    let (call_id, id) = path.into_inner();
    let data = state
        .db(move |c| {
            settings::document(c, ctx.organization_id, call_id, "call")?;
            let d = settings::document(c, ctx.organization_id, id, "recording")?;
            if d.parent_id != Some(call_id) {
                return Err(ApiError::missing());
            }
            Ok(d.data)
        })
        .await?;
    if data["state"] != "ready" && data["ready"] != true {
        return Err(ApiError::new(
            404,
            "recording_unavailable",
            "Запись пока недоступна",
        ));
    }
    let relative = data["path"].as_str().ok_or_else(ApiError::missing)?;
    let root = state
        .media_root
        .canonicalize()
        .map_err(|_| ApiError::missing())?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| ApiError::missing())?;
    if !path.starts_with(root.join(ctx.organization_id.to_string())) {
        return Err(ApiError::missing());
    }
    let mut file = actix_files::NamedFile::open_async(path)
        .await
        .map_err(|_| ApiError::missing())?
        .disable_content_disposition();
    let mime = data["mime"].as_str().unwrap_or("audio/wav");
    if let Ok(mime) = mime.parse() {
        file = file.set_content_type(mime);
    }
    Ok(file.into_response(&req))
}
async fn callbacks(
    state: web::Data<AppState>,
    req: HttpRequest,
    q: web::Query<Query>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    let q = q.into_inner();
    Ok(ok(state
        .db(move |c| {
            let all = all_calls(c, ctx.organization_id)?;
            let now = Utc::now();
            let p = policy(c, ctx.organization_id)?;
            let episodes = domain::callback_episodes(&all, &p, now);
            let selected = filtered(all, &q, &p)?
                .into_iter()
                .filter_map(|v| v["id"].as_str().map(str::to_owned).map(|id| (id, v)))
                .collect::<HashMap<_, _>>();
            let items = episodes
                .into_iter()
                .filter_map(|mut e| {
                    selected.get(e["call_id"].as_str()?).map(|call| {
                        e["callback_age_minutes"] = call["callback_age_minutes"].clone();
                        e["callback_attempts"] = call["callback_attempts"].clone();
                        e
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({"items":items,"as_of":now}))
        })
        .await?))
}
async fn dashboard(
    state: web::Data<AppState>,
    req: HttpRequest,
    q: web::Query<Query>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    let q = q.into_inner();
    Ok(ok(state.db(move|c|{
 let p=policy(c,ctx.organization_id)?;let all=all_calls(c,ctx.organization_id)?;let cov=coverage(&all);let all_episodes=domain::callback_episodes(&all,&p,Utc::now());let calls=filtered(all,&q,&p)?;
 let selected=calls.iter().filter_map(|v|v["id"].as_str()).collect::<std::collections::HashSet<_>>();let episodes=all_episodes.into_iter().filter(|e|e["call_id"].as_str().is_some_and(|id|selected.contains(id))).collect::<Vec<_>>();let mut days:BTreeMap<String,(usize,usize)>=BTreeMap::new();let mut employees:BTreeMap<String,(usize,f64,usize)>=BTreeMap::new();let mut scores=[0usize;5];let mut sum=0.;let mut scored=0usize;
 let tz=p["timezone"].as_str().unwrap_or("Europe/Moscow").parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::Europe::Moscow);
 for call in &calls{let day=at(call).with_timezone(&tz).format("%Y-%m-%d").to_string();let day=days.entry(day).or_default();day.0+=1;if call["direction"]=="in"&&!domain::human_connected(call){day.1+=1;}
 let employee=employees.entry(call["employee"].as_str().unwrap_or("Не определён").to_owned()).or_default();employee.0+=1;
 if let Some(score)=call["score"].as_f64().filter(|s|s.is_finite()&&(0.0..=100.).contains(s)){sum+=score;scored+=1;employee.1+=score;employee.2+=1;scores[((score/20.).floor() as usize).min(4)]+=1;}}
 let daily=days.into_iter().map(|(date,(calls,missed))|json!({"date":date,"calls":calls,"missed":missed})).collect::<Vec<_>>();let employees=employees.into_iter().map(|(name,(calls,sum,count))|json!({"name":name,"calls":calls,"average_score":if count>0{Some((sum/count as f64*10.).round()/10.)}else{None}})).collect::<Vec<_>>();
 let open=episodes.iter().filter(|e|e["state"]=="open"||e["state"]=="overdue").count();let overdue=episodes.iter().filter(|e|e["state"]=="overdue").count();
 Ok(json!({"totals":{"calls":calls.len(),"answered":calls.iter().filter(|c|domain::human_connected(c)).count(),"missed":calls.iter().filter(|c|c["direction"]=="in"&&!domain::human_connected(c)).count(),"unhandled":open,"overdue":overdue,"analyzed":calls.iter().filter(|call|call["analysis_state"]=="analyzed"||call["analysis_state"]=="partial").count(),"average_score":if scored>0{Some((sum/scored as f64*10.).round()/10.)}else{None}},"daily":daily,"employees":employees,"scores":scores.iter().enumerate().map(|(i,n)|json!({"bucket":format!("{}–{}",i*20,(i+1)*20),"count":n})).collect::<Vec<_>>(),"coverage":cov}))
 }).await?))
}
async fn confirm_roles(
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let (call_id, transcript_id) = path.into_inner();
    let roles = body["roles"]
        .as_object()
        .filter(|m| !m.is_empty() && m.len() <= 16)
        .ok_or_else(|| ApiError::bad("Назначьте роли участников"))?
        .clone();
    for role in roles.values() {
        if !["manager", "client", "unknown"].contains(&role.as_str().unwrap_or("")) {
            return Err(ApiError::bad("Неверная роль участника"));
        }
    }
    Ok(ok(state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                diesel::sql_query(
                    "SELECT id FROM documents WHERE organization_id=$1 AND id=$2 FOR UPDATE",
                )
                .bind::<diesel::sql_types::Uuid, _>(ctx.organization_id)
                .bind::<diesel::sql_types::Uuid, _>(call_id)
                .execute(c)?;
                settings::document(c, ctx.organization_id, call_id, "call")?;
                let original =
                    settings::document(c, ctx.organization_id, transcript_id, "transcript")?;
                if original.parent_id != Some(call_id) {
                    return Err(ApiError::missing());
                }
                let mut data = original.data;
                let segments = data["segments"]
                    .as_array_mut()
                    .ok_or_else(|| ApiError::bad("В расшифровке нет сегментов"))?;
                let speakers = segments
                    .iter()
                    .filter_map(|s| s["speaker"].as_str().map(str::to_owned))
                    .collect::<std::collections::HashSet<_>>();
                if roles.keys().any(|key| !speakers.contains(key)) {
                    return Err(ApiError::bad("Участник отсутствует в расшифровке"));
                }
                for segment in segments.iter_mut() {
                    if let Some(role) = segment["speaker"].as_str().and_then(|v| roles.get(v)) {
                        segment["role"] = role.clone();
                    }
                }
                let known = segments
                    .iter()
                    .all(|s| matches!(s["role"].as_str(), Some("manager" | "client")));
                let id = Uuid::new_v5(
                    &transcript_id,
                    serde_json::to_string(&roles)
                        .map_err(|_| ApiError::bad("Неверные роли"))?
                        .as_bytes(),
                );
                let existing = persistence::get_document(c, ctx.organization_id, id)?;
                let doc = if let Some(doc) = existing {
                    doc
                } else {
                    let count = persistence::list_children(
                        c,
                        ctx.organization_id,
                        call_id,
                        Some("transcript"),
                    )?
                    .len();
                    data["version"] = json!(count + 1);
                    data["transcript_version"] = json!(count + 1);
                    data["role_confidence"] = json!(if known { "known" } else { "uncertain" });
                    data["role_source"] = json!("human_confirmed");
                    data["role_confirmed_by"] = json!(ctx.user_id);
                    data["source_transcript_id"] = json!(transcript_id);
                    data["created_at"] = json!(Utc::now());
                    persistence::save_document(
                        c,
                        ctx.organization_id,
                        id,
                        "transcript",
                        Some(call_id),
                        data,
                        None,
                    )?
                };
                let job = persistence::enqueue(
                    c,
                    ctx.organization_id,
                    "analyze",
                    Some(call_id),
                    &format!("transcript:{id}"),
                    json!({"transcript_id":id}),
                )?;
                settings::audit(
                    c,
                    &ctx,
                    "transcript.roles_confirmed",
                    Some(call_id),
                    json!({"transcript_id":id,"roles":roles}),
                )?;
                Ok(json!({"transcript":to_json(doc),"job_id":job.id}))
            })
        })
        .await?))
}
pub fn configure(c: &mut web::ServiceConfig) {
    c.route(
        "/api/calls/{call_id}/transcripts/{transcript_id}/roles",
        web::post().to(confirm_roles),
    )
    .route("/api/calls", web::get().to(list))
    .route("/api/calls/{id}", web::get().to(detail))
    .route("/api/calls/{id}", web::patch().to(annotate))
    .route("/api/calls/{id}", web::delete().to(delete))
    .route("/api/calls/{id}/reprocess", web::post().to(reprocess))
    .route(
        "/api/calls/{call_id}/media/{recording_id}",
        web::get().to(media),
    )
    .route("/api/callbacks", web::get().to(callbacks))
    .route("/api/dashboard", web::get().to(dashboard));
}
