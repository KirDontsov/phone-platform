use crate::*;
use actix_web::{http::header, HttpRequest};
use diesel::{
    prelude::*,
    sql_types::{Jsonb, Text, Timestamptz, Uuid as SqlUuid},
};
use serde::Deserialize;

pub fn connection_id(org: Uuid, kind: &str) -> Uuid {
    Uuid::new_v5(&org, format!("connection:{kind}").as_bytes())
}
#[derive(QueryableByName)]
struct OAuthGrant {
    #[diesel(sql_type=SqlUuid)]
    id: Uuid,
    #[diesel(sql_type=SqlUuid)]
    organization_id: Uuid,
    #[diesel(sql_type=Jsonb)]
    payload: Value,
}
fn safe_job(job: persistence::Job) -> Value {
    let mut v = serde_json::to_value(job).unwrap_or(json!({}));
    if let Some(o) = v.as_object_mut() {
        o.remove("payload");
    }
    v
}
async fn operations(
    state: web::Data<AppState>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 2).await?;
    let root = state.media_root.clone();
    let data=state.db(move|c|{
 let jobs=persistence::list_jobs(c,ctx.organization_id,200)?.into_iter().map(safe_job).collect::<Vec<_>>();
 let syncs=persistence::list_documents(c,ctx.organization_id,"sync_run")?.into_iter().map(to_json).take(50).collect::<Vec<_>>();
 let deliveries=persistence::list_documents(c,ctx.organization_id,"delivery")?.into_iter().map(to_json).take(100).collect::<Vec<_>>();
 let audit=persistence::list_documents(c,ctx.organization_id,"audit")?.into_iter().map(to_json).take(100).collect::<Vec<_>>();
 let analyses=persistence::list_documents(c,ctx.organization_id,"analysis")?;let mut usage=json!({"input_tokens":0,"output_tokens":0,"cost":0.0,"runs":analyses.len()});
 let mut input=0i64;let mut output=0i64;let mut cost=0.;let mut unknown_cost_runs=0;for a in analyses{input+=a.data["usage"]["input_tokens"].as_i64().unwrap_or(0);output+=a.data["usage"]["output_tokens"].as_i64().unwrap_or(0);if let Some(actual)=a.data["usage"]["cost"].as_f64(){cost+=actual;}else{unknown_cost_runs+=1;}}
 usage["input_tokens"]=json!(input);usage["output_tokens"]=json!(output);usage["cost"]=if unknown_cost_runs==0{json!(cost)}else{Value::Null};usage["known_cost"]=json!(cost);usage["unknown_cost_runs"]=json!(unknown_cost_runs);
 Ok(json!({"jobs":jobs,"sync_runs":syncs,"deliveries":deliveries,"audit":audit,"usage":usage}))
 }).await?;
    let heartbeat = tokio::fs::read(root.join(".worker-health.json"))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(json!({"state":"unknown"}));
    let mut data = data;
    data["health"] = json!({"database":"ready","worker":heartbeat,"demo_mode":state.demo});
    Ok(ok(data))
}
async fn audit_list(
    state: web::Data<AppState>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 2).await?;
    Ok(ok(state.db(move|c|Ok(json!({"items":persistence::list_documents(c,ctx.organization_id,"audit")?.into_iter().map(to_json).take(500).collect::<Vec<_>>()}))).await?))
}
async fn retry(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let id = *id;
    state
        .db(move |c| {
            if !persistence::retry_job(c, ctx.organization_id, id)? {
                return Err(ApiError::bad(
                    "Задание нельзя повторить в текущем состоянии",
                ));
            }
            settings::audit(c, &ctx, "job.retry", Some(id), json!({}))?;
            Ok(())
        })
        .await?;
    Ok(ok(json!({"ok":true})))
}
#[derive(QueryableByName)]
struct RetentionPolicy {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
async fn purge(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    Ok(ok(state.db(move|c|{let policy=diesel::sql_query("SELECT jsonb_build_object('retention_days',retention_days) AS data FROM organizations WHERE id=$1").bind::<SqlUuid,_>(ctx.organization_id).get_result::<RetentionPolicy>(c)?;let job=persistence::enqueue(c,ctx.organization_id,"retention",None,&format!("retention:{}:{}",ctx.organization_id,Utc::now().format("%Y-%m-%dT%H")),policy.data)?;settings::audit(c,&ctx,"retention.requested",Some(job.id),json!({}))?;Ok(json!({"job_id":job.id}))}).await?))
}
async fn integrations(
    state: web::Data<AppState>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    Ok(ok(state.db(move|c|Ok(json!({"links":persistence::list_documents(c,ctx.organization_id,"external_link")?.into_iter().map(to_json).collect::<Vec<_>>(),"deliveries":persistence::list_documents(c,ctx.organization_id,"delivery")?.into_iter().map(to_json).take(200).collect::<Vec<_>>()}))).await?))
}
async fn link(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let b = body.into_inner();
    let call_id = id_field(&b, "call_id")?;
    if b["provider"] != "amocrm" {
        return Err(ApiError::bad("Поддерживается связь с amoCRM"));
    }
    for key in ["contact_id", "deal_id"] {
        if b[key]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 50 && s.bytes().all(|b| b.is_ascii_digit()))
            .is_none()
        {
            return Err(ApiError::bad("Выберите контакт и сделку"));
        }
    }
    Ok(ok(state.db(move|c|{settings::document(c,ctx.organization_id,call_id,"call")?;let id=Uuid::new_v5(&call_id,b"external_link:amocrm");let old=persistence::get_document(c,ctx.organization_id,id)?;
 let d=persistence::save_document(c,ctx.organization_id,id,"external_link",Some(call_id),json!({"call_id":call_id,"provider":"amocrm","contact_id":b["contact_id"],"deal_id":b["deal_id"]}),old.map(|d|d.version))?;settings::audit(c,&ctx,"integration.linked",Some(call_id),json!({"provider":"amocrm"}))?;Ok(to_json(d))}).await?))
}
async fn export(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let provider = str_field(&body, "provider")?.to_owned();
    if !["amocrm", "sheets"].contains(&provider.as_str()) {
        return Err(ApiError::bad("Неизвестная интеграция"));
    }
    let ids = body["call_ids"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 100)
        .ok_or_else(|| ApiError::bad("Выберите от 1 до 100 звонков"))?
        .iter()
        .map(|v| {
            v.as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| ApiError::bad("Неверный ID звонка"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ok(state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                let mut jobs = vec![];
                for id in ids {
                    let call = settings::document(c, ctx.organization_id, id, "call")?;
                    let key = format!("export:{provider}:{id}:{}", call.version);
                    let job = persistence::enqueue(
                        c,
                        ctx.organization_id,
                        "export",
                        Some(id),
                        &key,
                        json!({"provider":provider,"call_version":call.version}),
                    )?;
                    jobs.push(job.id);
                }
                settings::audit(
                    c,
                    &ctx,
                    "integration.export_requested",
                    None,
                    json!({"provider":provider,"count":jobs.len()}),
                )?;
                Ok(json!({"job_ids":jobs}))
            })
        })
        .await?))
}
async fn authorize(
    state: web::Data<AppState>,
    req: HttpRequest,
    provider: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 2).await?;
    let provider = provider.into_inner();
    if !["amocrm", "sheets"].contains(&provider.as_str()) {
        return Err(ApiError::missing());
    }
    let key = state.key;
    let k = provider.clone();
    let token = auth::random_token();
    let digest = auth::hash(&token);
    let config=state.db(move|c|{let d=settings::document(c,ctx.organization_id,connection_id(ctx.organization_id,&k),"connection")?;let config=settings::config_value(&d,&key)?;
 diesel::sql_query("INSERT INTO token_grants(token_hash,kind,organization_id,user_id,payload,expires_at) VALUES($1,'oauth',$2,$3,$4,$5)")
 .bind::<Text,_>(digest).bind::<SqlUuid,_>(ctx.organization_id).bind::<SqlUuid,_>(ctx.user_id).bind::<Jsonb,_>(json!({"provider":k,"session_hash":ctx.token_hash,"connection_id":d.id,"connection_version":d.version})).bind::<Timestamptz,_>(Utc::now()+chrono::Duration::minutes(10)).execute(c)?;Ok(config)}).await?;
    let url = phone_worker::oauth_authorize_url(&provider, &config, &token).map_err(|e| {
        ApiError::new(
            422,
            "oauth_configuration",
            format!("Проверьте параметры интеграции: {}", e.code),
        )
    })?;
    Ok(ok(json!({"url":url})))
}
#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}
async fn callback(
    state: web::Data<AppState>,
    req: HttpRequest,
    provider: web::Path<String>,
    query: web::Query<Callback>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 2).await?;
    let provider = provider.into_inner();
    if !["amocrm", "sheets"].contains(&provider.as_str()) {
        return Err(ApiError::missing());
    }
    if query.error.is_some() {
        return Err(ApiError::bad("Авторизация отменена. Повторите подключение"));
    }
    let code = query
        .code
        .clone()
        .filter(|v| v.len() < 8192)
        .ok_or_else(|| ApiError::bad("Отсутствует OAuth code"))?;
    let digest = auth::hash(
        query
            .state
            .as_deref()
            .ok_or_else(|| ApiError::bad("Отсутствует OAuth state"))?,
    );
    let key = state.key;
    let k = provider.clone();
    let (mut config,conn_id,version)=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  let grant=diesel::sql_query("SELECT id,organization_id,payload FROM token_grants WHERE token_hash=$1 AND kind='oauth' AND consumed_at IS NULL AND expires_at>now() FOR UPDATE").bind::<Text,_>(digest).get_result::<OAuthGrant>(c).optional()?.ok_or_else(||ApiError::bad("OAuth state истёк или уже использован"))?;
  if grant.organization_id!=ctx.organization_id||grant.payload["provider"]!=k||grant.payload["session_hash"]!=ctx.token_hash{return Err(ApiError::forbidden());}
  let id=grant.payload["connection_id"].as_str().and_then(|s|Uuid::parse_str(s).ok()).ok_or_else(ApiError::missing)?;let d=settings::document(c,ctx.organization_id,id,"connection")?;
  if d.version!=grant.payload["connection_version"].as_i64().unwrap_or(-1){return Err(ApiError::new(409,"connection_changed","Подключение изменилось. Начните авторизацию заново"));}
  diesel::sql_query("UPDATE token_grants SET consumed_at=now() WHERE id=$1").bind::<SqlUuid,_>(grant.id).execute(c)?;Ok((settings::config_value(&d,&key)?,id,d.version))
 })).await?;
    let tokens = phone_worker::oauth_exchange(&provider, &config, &code)
        .await
        .map_err(|e| {
            ApiError::new(
                422,
                "oauth_exchange_failed",
                format!("Авторизация не завершена: {}", e.code),
            )
        })?;
    if let Some(map) = tokens.as_object() {
        for (k, v) in map {
            config[k] = v.clone();
        }
    }
    if !config["expires_at"].is_i64() {
        config["expires_at"] =
            json!(Utc::now().timestamp() + config["expires_in"].as_i64().unwrap_or(3600));
    }
    let ctx = auth::context(&state, &req, false, 2).await?;
    let encrypted = contracts::seal(&config, &state.key).map_err(|_| ApiError::internal())?;
    state
        .db(move |c| {
            let mut d = settings::document(c, ctx.organization_id, conn_id, "connection")?;
            if d.version != version {
                return Err(ApiError::new(
                    409,
                    "connection_changed",
                    "Подключение изменилось",
                ));
            }
            d.data["config_encrypted"] = json!(encrypted);
            d.data["state"] = json!("configured");
            d.data["last_error"] = Value::Null;
            persistence::save_document(
                c,
                ctx.organization_id,
                conn_id,
                "connection",
                None,
                d.data,
                Some(version),
            )?;
            settings::audit(
                c,
                &ctx,
                "integration.authorized",
                Some(conn_id),
                json!({"provider":provider}),
            )?;
            Ok(())
        })
        .await?;
    Ok(HttpResponse::SeeOther()
        .insert_header((
            header::LOCATION,
            format!("{}/settings?tab=integrations", state.public_url),
        ))
        .finish())
}
#[derive(Deserialize)]
struct Search {
    phone: String,
}
async fn search_deals(
    state: web::Data<AppState>,
    req: HttpRequest,
    q: web::Query<Search>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 1).await?;
    if q.phone.len() > 40 {
        return Err(ApiError::bad("Неверный номер"));
    }
    let key = state.key;
    let config = state
        .db(move |c| {
            let d = settings::document(
                c,
                ctx.organization_id,
                connection_id(ctx.organization_id, "amocrm"),
                "connection",
            )?;
            settings::config_value(&d, &key)
        })
        .await?;
    let result = phone_worker::amocrm_search(&config, &q.phone)
        .await
        .map_err(|e| {
            ApiError::new(
                422,
                "crm_search_failed",
                format!("Поиск не выполнен: {}", e.code),
            )
        })?;
    Ok(ok(result))
}
pub fn configure(c: &mut web::ServiceConfig) {
    c.route("/api/operations", web::get().to(operations))
        .route("/api/operations/audit", web::get().to(audit_list))
        .route("/api/operations/jobs/{id}/retry", web::post().to(retry))
        .route("/api/operations/purge", web::post().to(purge))
        .route("/api/integrations", web::get().to(integrations))
        .route("/api/integrations/link", web::post().to(link))
        .route("/api/integrations/export", web::post().to(export))
        .route(
            "/api/integrations/amocrm/search",
            web::get().to(search_deals),
        )
        .route(
            "/api/integrations/{provider}/authorize",
            web::get().to(authorize),
        )
        .route(
            "/api/integrations/{provider}/callback",
            web::get().to(callback),
        );
}
