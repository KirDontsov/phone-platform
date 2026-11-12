use crate::*;
use actix_web::HttpRequest;
use diesel::{
    prelude::*,
    sql_types::{Integer, Jsonb, Text, Uuid as SqlUuid},
};

pub fn document(
    conn: &mut diesel::PgConnection,
    org: Uuid,
    id: Uuid,
    kind: &str,
) -> Result<Document, ApiError> {
    let d = persistence::get_document(conn, org, id)?
        .filter(|d| d.kind == kind && !d.data["deleted"].as_bool().unwrap_or(false))
        .ok_or_else(ApiError::missing)?;
    Ok(d)
}
pub fn audit(
    conn: &mut diesel::PgConnection,
    ctx: &auth::Context,
    action: &str,
    subject: Option<Uuid>,
    details: Value,
) -> Result<(), ApiError> {
    persistence::save_document(
        conn,
        ctx.organization_id,
        Uuid::new_v4(),
        "audit",
        None,
        json!({"action":action,"user_id":ctx.user_id,"user_name":ctx.name,"subject_id":subject,"details":details,"created_at":Utc::now()}),
        None,
    )?;
    Ok(())
}
fn color(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
async fn taxonomy(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    Ok(ok(state.db(move|c|Ok(json!({"tags":persistence::list_documents(c,ctx.organization_id,"tag")?.into_iter().map(to_json).collect::<Vec<_>>(),"statuses":persistence::list_documents(c,ctx.organization_id,"status")?.into_iter().map(to_json).collect::<Vec<_>>()}))).await?))
}
async fn create_taxonomy(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let b = body.into_inner();
    Ok(ok(state
        .db(move |c| create_taxonomy_in_db(c, &ctx, Uuid::new_v4(), b))
        .await?))
}
async fn patch_taxonomy(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let id = *id;
    let b = body.into_inner();
    Ok(ok(state
        .db(move |c| patch_taxonomy_in_db(c, &ctx, id, b))
        .await?))
}
async fn rubrics(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    Ok(ok(state.db(move|c|Ok(json!({"items":persistence::list_documents(c,ctx.organization_id,"rubric")?.into_iter().map(to_json).collect::<Vec<_>>()}))).await?))
}
async fn create_rubric(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let b = body.into_inner();
    Ok(ok(state
        .db(move |c| create_rubric_in_db(c, &ctx, Uuid::new_v4(), b))
        .await?))
}
async fn activate_rubric(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let id = *id;
    state
        .db(move |c| activate_rubric_in_db(c, &ctx, id))
        .await?;
    Ok(ok(json!({"ok":true})))
}
async fn filters(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 0).await?;
    Ok(ok(state.db(move|c|Ok(json!({"items":persistence::list_documents(c,ctx.organization_id,"filter")?.into_iter().map(to_json).collect::<Vec<_>>()}))).await?))
}
async fn create_filter(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let b = body.into_inner();
    Ok(ok(state
        .db(move |c| save_filter_in_db(c, &ctx, Uuid::new_v4(), b, None))
        .await?))
}
async fn delete_filter(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 1).await?;
    let id = *id;
    state
        .db(move |c| {
            settings_change(c, &ctx, None, |c| {
                document(c, ctx.organization_id, id, "filter")?;
                persistence::delete_document(c, ctx.organization_id, id)?;
                audit(c, &ctx, "filter.deleted", Some(id), json!({}))?;
                Ok(())
            })
        })
        .await?;
    Ok(ok(json!({"ok":true})))
}

const CONNECTION_KINDS: [&str; 5] = ["telphin", "amocrm", "sheets", "asr", "analysis"];
fn connection_id(org: Uuid, kind: &str) -> Uuid {
    Uuid::new_v5(&org, format!("connection:{kind}").as_bytes())
}
pub fn public_connection(mut d: Document) -> Value {
    d.data.as_object_mut().map(|o| o.remove("config_encrypted"));
    to_json(d)
}
pub fn config_value(d: &Document, key: &[u8; 32]) -> Result<Value, ApiError> {
    contracts::unseal(
        d.data["config_encrypted"]
            .as_str()
            .ok_or_else(|| ApiError::bad("Подключение не настроено"))?,
        key,
    )
    .map_err(|_| ApiError::internal())
}
fn redacted(config: &Value) -> Value {
    let mut c = config.clone();
    if let Some(map) = c.as_object_mut() {
        for (key, v) in map.iter_mut() {
            if [
                "api_key",
                "app_secret",
                "client_secret",
                "access_token",
                "refresh_token",
                "password",
            ]
            .contains(&key.as_str())
            {
                *v = if v.as_str().is_some_and(|v| !v.is_empty()) {
                    json!("<configured>")
                } else {
                    Value::Null
                };
            }
        }
    }
    c
}
async fn connections(
    state: web::Data<AppState>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, false, 2).await?;
    Ok(ok(state.db(move|c|Ok(json!({"items":persistence::list_documents(c,ctx.organization_id,"connection")?.into_iter().map(public_connection).collect::<Vec<_>>()}))).await?))
}
pub fn save_connection_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    kind: String,
    config: Value,
    key: [u8; 32],
    public_url: String,
    demo: bool,
    runner_password: Option<String>,
) -> Result<Value, ApiError> {
    if !CONNECTION_KINDS.contains(&kind.as_str()) {
        return Err(ApiError::bad("Неизвестное подключение"));
    }
    if !config.is_object() || config.to_string().len() > 32768 {
        return Err(ApiError::bad("Неверная конфигурация"));
    }
    settings_change(c, ctx, None, |c| {
        let id = connection_id(ctx.organization_id, &kind);
        let old = persistence::get_document(c, ctx.organization_id, id)?;
        let mut merged = if let Some(d) = &old {
            config_value(d, &key).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };
        for (k, v) in config.as_object().unwrap() {
            let secret = [
                "api_key",
                "app_secret",
                "client_secret",
                "access_token",
                "refresh_token",
                "password",
            ]
            .contains(&k.as_str());
            if secret
                && (v.is_null() || v.as_str() == Some("") || v.as_str() == Some("<configured>"))
            {
                continue;
            }
            if ["access_token", "refresh_token"].contains(&k.as_str()) {
                return Err(ApiError::bad(
                    "OAuth токены получает сервер после авторизации",
                ));
            }
            merged[k] = v.clone();
        }
        if kind == "telphin" {
            if !["apiproxy.telphin.ru", "altapi.telphin.ru"]
                .contains(&merged["host"].as_str().unwrap_or("apiproxy.telphin.ru"))
            {
                return Err(ApiError::bad("Неверный API хост Телфина"));
            }
            let cid = merged["client_id"].as_str().or_else(|| None).unwrap_or("");
            if cid.is_empty() || !cid.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ApiError::bad("Укажите числовой ID клиента АТС"));
            }
        }
        if ["asr", "analysis"].contains(&kind.as_str()) && merged["provider"] == "demo" && !demo {
            return Err(ApiError::bad("Demo provider доступен только в DEMO_MODE"));
        }
        if ["amocrm", "sheets"].contains(&kind.as_str()) {
            let redirect = format!("{public_url}/api/integrations/{kind}/callback");
            if let Some(r) = merged["redirect_uri"].as_str() {
                if !r.is_empty() && r != redirect {
                    return Err(ApiError::bad(
                        "Redirect URI должен совпадать с callback этого сервиса",
                    ));
                }
            }
            merged["redirect_uri"] = json!(redirect);
        }
        if kind == "sheets" {
            phone_worker::integrations::sheets_columns(&merged, &json!({}), None, "", "")
                .map_err(|_| ApiError::bad("Проверь соответствие полей колонкам: ключ звонка в A, разные колонки от B до AZ для остальных полей"))?;
        }
        if kind == "analysis"
            && merged["provider"] == "opencode"
            && merged["base_url"].as_str() == Some("http://model-runner:4096")
            && merged["api_key"].as_str().is_none_or(|v| v.is_empty())
        {
            if let Some(password) = runner_password {
                merged["api_key"] = json!(password);
            }
        }
        let encrypted = contracts::seal(&merged, &key).map_err(|_| ApiError::internal())?;
        let data = json!({"kind":kind,"state":"configured","config":redacted(&merged),"config_encrypted":encrypted,"last_error":null,"last_sync_at":old.as_ref().and_then(|v|v.data.get("last_sync_at")).cloned().unwrap_or(Value::Null)});
        let d = persistence::save_document(
            c,
            ctx.organization_id,
            id,
            "connection",
            None,
            data,
            old.map(|v| v.version),
        )?;
        audit(c, &ctx, "connection.saved", Some(id), json!({"kind":kind}))?;
        Ok(public_connection(d))
    })
}
async fn save_connection(
    state: web::Data<AppState>,
    req: HttpRequest,
    kind: web::Path<String>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let kind = kind.into_inner();
    let mut config = body["config"].clone();
    let mut expected_version = None;
    let resolving = kind == "telphin" && crate::telphin_identity::needs_resolution(&config)?;
    if resolving {
        let org = ctx.organization_id;
        let key = state.key;
        let previous = state
            .db(move |c| {
                Ok(persistence::get_document(
                    c,
                    org,
                    connection_id(org, "telphin"),
                )?)
            })
            .await?;
        expected_version = previous.as_ref().map(|d| d.version);
        let mut merged = previous
            .as_ref()
            .map(|d| config_value(d, &key))
            .transpose()?
            .unwrap_or(json!({}));
        for (k, v) in config
            .as_object()
            .ok_or_else(|| ApiError::bad("Нужна конфигурация подключения"))?
        {
            if k == "app_secret"
                && (v.is_null() || v.as_str() == Some("") || v.as_str() == Some("<configured>"))
            {
                continue;
            }
            merged[k] = v.clone();
        }
        merged.as_object_mut().unwrap().remove("client_id");
        config = crate::telphin_identity::complete(&req, merged).await?;
    }
    let key = state.key;
    let public_url = state.public_url.clone();
    let demo = state.demo;
    let password = std::env::var("MODEL_RUNNER_PASSWORD").ok();
    Ok(ok(state
        .db(move |c| c.transaction::<_,ApiError,_>(|c| {
            if resolving {
                lock_settings_revision(c,ctx.organization_id)?;
                let _=diesel::sql_query("SELECT s.user_id,s.organization_id,m.role,s.csrf_token,u.email,u.name,s.token_hash FROM sessions s JOIN memberships m ON m.user_id=s.user_id AND m.organization_id=s.organization_id JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.organization_id=$2 AND s.user_id=$3 AND s.expires_at>now() AND m.role='owner' FOR SHARE OF s,m").bind::<Text,_>(&ctx.token_hash).bind::<SqlUuid,_>(ctx.organization_id).bind::<SqlUuid,_>(ctx.user_id).get_result::<auth::Context>(c).optional()?.ok_or_else(ApiError::forbidden)?;
                let current=persistence::get_document(c,ctx.organization_id,connection_id(ctx.organization_id,"telphin"))?;
                if current.as_ref().map(|d|d.version)!=expected_version {return Err(ApiError::new(409,"connection_changed","Подключение изменилось. Обновите страницу и повторите сохранение"));}
            }
            save_connection_in_db(c,&ctx,kind,config,key,public_url,demo,password)
        }))
        .await?))
}
async fn disconnect(
    state: web::Data<AppState>,
    req: HttpRequest,
    kind: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let kind = kind.into_inner();
    if !CONNECTION_KINDS.contains(&kind.as_str()) {
        return Err(ApiError::bad("Неизвестное подключение"));
    }
    state
        .db(move |c| {
            settings_change(c, &ctx, None, |c| {
                let id = connection_id(ctx.organization_id, &kind);
                if let Some(mut d) = persistence::get_document(c, ctx.organization_id, id)? {
                    d.data =
                        json!({"kind":kind,"state":"disconnected","config":{},"last_error":null});
                    persistence::save_document(
                        c,
                        ctx.organization_id,
                        id,
                        "connection",
                        None,
                        d.data,
                        Some(d.version),
                    )?;
                }
                audit(
                    c,
                    &ctx,
                    "connection.disconnected",
                    Some(id),
                    json!({"kind":kind}),
                )?;
                Ok(())
            })
        })
        .await?;
    Ok(ok(json!({"ok":true})))
}
async fn test_connection(
    state: web::Data<AppState>,
    req: HttpRequest,
    kind: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let kind = kind.into_inner();
    let key = state.key;
    let k = kind.clone();
    let config = state
        .db(move |c| {
            let d = document(
                c,
                ctx.organization_id,
                connection_id(ctx.organization_id, &k),
                "connection",
            )?;
            config_value(&d, &key)
        })
        .await?;
    let result = phone_worker::test_connection(&kind, &config)
        .await
        .map_err(|e| {
            ApiError::new(
                422,
                "provider_test_failed",
                format!("Проверка не пройдена: {}", e.code),
            )
        })?;
    Ok(ok(result))
}
async fn sync(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let payload = body.into_inner();
    let key = state.key;
    Ok(ok(state
        .db(move |c| {
            let id = connection_id(ctx.organization_id, "telphin");
            let d = document(c, ctx.organization_id, id, "connection")?;
            config_value(&d, &key)?;
            let dedup = format!("sync:{id}:{}", Uuid::new_v4());
            let job =
                persistence::enqueue(c, ctx.organization_id, "sync", Some(id), &dedup, payload)?;
            audit(c, &ctx, "sync.requested", Some(job.id), json!({}))?;
            Ok(json!({"job_id":job.id}))
        })
        .await?))
}
#[derive(QueryableByName)]
struct OrgData {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
pub fn patch_org_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    b: Value,
) -> Result<Value, ApiError> {
    settings_change(c, ctx, None, |c| {
        let old = diesel::sql_query(
            "SELECT to_jsonb(o) AS data FROM organizations o WHERE id=$1 FOR UPDATE",
        )
        .bind::<SqlUuid, _>(ctx.organization_id)
        .get_result::<OrgData>(c)?
        .data;
        let name = b["name"]
            .as_str()
            .unwrap_or(old["name"].as_str().unwrap_or("Компания"))
            .trim()
            .to_owned();
        let timezone = b["timezone"]
            .as_str()
            .unwrap_or(old["timezone"].as_str().unwrap_or("Europe/Moscow"))
            .to_owned();
        let minutes = b["callback_minutes"]
            .as_i64()
            .unwrap_or(old["callback_minutes"].as_i64().unwrap_or(120));
        let retention = b["retention_days"]
            .as_i64()
            .unwrap_or(old["retention_days"].as_i64().unwrap_or(90));
        if name.is_empty()
            || name.len() > 240
            || !(1..=10080).contains(&minutes)
            || !(1..=3650).contains(&retention)
        {
            return Err(ApiError::bad("Проверьте параметры компании"));
        }
        if timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(ApiError::bad("Неизвестный часовой пояс"));
        }
        let mut extra = old["data"].clone();
        if !extra.is_object() {
            extra = json!({});
        }
        for key in [
            "working_hours",
            "callback_allow_inbound",
            "callback_min_duration",
        ] {
            if let Some(v) = b.get(key) {
                extra[key] = v.clone();
            }
        }
        let mut checked = extra.clone();
        checked["timezone"] = json!(timezone);
        checked["callback_minutes"] = json!(minutes);
        domain::validate_callback_policy(&checked)
            .map_err(|_| ApiError::bad("Проверьте рабочее расписание и правила перезвона"))?;
        diesel::sql_query("UPDATE organizations SET name=$1,timezone=$2,callback_minutes=$3,retention_days=$4,data=$5 WHERE id=$6").bind::<Text,_>(name).bind::<Text,_>(timezone).bind::<Integer,_>(minutes as i32).bind::<Integer,_>(retention as i32).bind::<Jsonb,_>(extra).bind::<SqlUuid,_>(ctx.organization_id).execute(c)?;
        audit(
            c,
            &ctx,
            "organization.updated",
            Some(ctx.organization_id),
            json!({}),
        )?;
        Ok(diesel::sql_query("SELECT jsonb_build_object('id',id,'name',name,'timezone',timezone,'callback_minutes',callback_minutes,'retention_days',retention_days)||data AS data FROM organizations WHERE id=$1").bind::<SqlUuid,_>(ctx.organization_id).get_result::<OrgData>(c)?.data)
    })
}
async fn patch_org(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = auth::context(&state, &req, true, 2).await?;
    let b = body.into_inner();
    Ok(ok(state.db(move |c| patch_org_in_db(c, &ctx, b)).await?))
}
pub fn configure(c: &mut web::ServiceConfig) {
    c.route("/api/taxonomy", web::get().to(taxonomy))
        .route("/api/taxonomy", web::post().to(create_taxonomy))
        .route("/api/taxonomy/{id}", web::patch().to(patch_taxonomy))
        .route("/api/rubrics", web::get().to(rubrics))
        .route("/api/rubrics", web::post().to(create_rubric))
        .route(
            "/api/rubrics/{id}/activate",
            web::post().to(activate_rubric),
        )
        .route("/api/filters", web::get().to(filters))
        .route("/api/filters", web::post().to(create_filter))
        .route("/api/filters/{id}", web::delete().to(delete_filter))
        .route("/api/connections", web::get().to(connections))
        .route("/api/connections/{kind}", web::put().to(save_connection))
        .route("/api/connections/{kind}", web::delete().to(disconnect))
        .route(
            "/api/connections/{kind}/test",
            web::post().to(test_connection),
        )
        .route("/api/connections/telphin/sync", web::post().to(sync))
        .route("/api/organization", web::patch().to(patch_org));
}

/// All HTTP and assistant settings writes share this revision/lock boundary.
pub fn lock_settings_revision(c: &mut diesel::PgConnection, org: Uuid) -> Result<i64, ApiError> {
    let row = diesel::sql_query("SELECT data FROM organizations WHERE id=$1 FOR NO KEY UPDATE")
        .bind::<SqlUuid, _>(org)
        .get_result::<OrgData>(c)?;
    Ok(row.data["settings_revision"].as_i64().unwrap_or(0))
}
pub fn settings_change<T, F>(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    expected: Option<i64>,
    f: F,
) -> Result<T, ApiError>
where
    F: FnOnce(&mut diesel::PgConnection) -> Result<T, ApiError>,
{
    c.transaction::<_,ApiError,_>(|c|{
  let revision=lock_settings_revision(c,ctx.organization_id)?;
  if expected.is_some_and(|v|v!=revision){return Err(ApiError::new(409,"settings_changed","Настройки изменились. Получите актуальные данные"));}
  let value=f(c)?;let next=revision.checked_add(1).ok_or_else(ApiError::internal)?;
  diesel::sql_query("UPDATE organizations SET data=jsonb_set(data,'{settings_revision}',to_jsonb($1::bigint),true) WHERE id=$2").bind::<diesel::sql_types::BigInt,_>(next).bind::<SqlUuid,_>(ctx.organization_id).execute(c)?;
  Ok(value)
 })
}
fn validate_taxonomy(data: &Value) -> Result<(), ApiError> {
    if data["name"]
        .as_str()
        .filter(|v| !v.trim().is_empty() && v.chars().count() <= 80)
        .is_none()
        || !color(data["color"].as_str().unwrap_or(""))
        || !data["archived"].is_boolean()
    {
        return Err(ApiError::bad("Проверьте название, цвет и архивный статус"));
    }
    Ok(())
}
pub fn create_taxonomy_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    id: Uuid,
    b: Value,
) -> Result<Value, ApiError> {
    let kind = str_field(&b, "kind")?.to_owned();
    if !["tag", "status"].contains(&kind.as_str()) {
        return Err(ApiError::bad("Неверный тип разметки"));
    }
    let data = json!({"name":str_field(&b,"name")?.trim(),"color":b["color"].as_str().unwrap_or("#0d9488"),"archived":b["archived"].as_bool().unwrap_or(false)});
    validate_taxonomy(&data)?;
    settings_change(c, ctx, None, |c| {
        if persistence::get_document(c, ctx.organization_id, id)?.is_some() {
            return Err(ApiError::new(
                409,
                "already_exists",
                "Такая настройка уже существует",
            ));
        }
        let d = persistence::save_document(c, ctx.organization_id, id, &kind, None, data, None)?;
        audit(c, ctx, "taxonomy.created", Some(id), json!({"kind":kind}))?;
        Ok(to_json(d))
    })
}
pub fn patch_taxonomy_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    id: Uuid,
    b: Value,
) -> Result<Value, ApiError> {
    settings_change(c, ctx, None, |c| {
        let mut d = persistence::get_document(c, ctx.organization_id, id)?
            .filter(|d| ["tag", "status"].contains(&d.kind.as_str()))
            .ok_or_else(ApiError::missing)?;
        for field in ["name", "color", "archived"] {
            if let Some(v) = b.get(field) {
                d.data[field] = v.clone();
            }
        }
        validate_taxonomy(&d.data)?;
        let version = if let Some(v) = b.get("version") {
            v.as_i64().ok_or_else(|| ApiError::bad("Неверная версия"))?
        } else {
            d.version
        };
        let updated = persistence::save_document(
            c,
            ctx.organization_id,
            id,
            &d.kind,
            None,
            d.data,
            Some(version),
        )?;
        audit(c, ctx, "taxonomy.updated", Some(id), json!({"kind":d.kind}))?;
        Ok(to_json(updated))
    })
}
pub fn save_filter_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    id: Uuid,
    b: Value,
    expected: Option<i64>,
) -> Result<Value, ApiError> {
    let name = str_field(&b, "name")?.trim().to_owned();
    let ast = b["ast"].clone();
    if name.len() > 200 || ast.to_string().len() > 32000 {
        return Err(ApiError::bad("Слишком большие название или фильтр"));
    }
    domain::validate_filter(&ast)
        .map_err(|_| ApiError::bad("Неверные поля или структура фильтра"))?;
    settings_change(c, ctx, None, |c| {
        if expected.is_some() {
            document(c, ctx.organization_id, id, "filter")?;
        } else if persistence::get_document(c, ctx.organization_id, id)?.is_some() {
            return Err(ApiError::new(
                409,
                "already_exists",
                "Фильтр уже существует",
            ));
        }
        let d = persistence::save_document(
            c,
            ctx.organization_id,
            id,
            "filter",
            None,
            json!({"name":name,"ast":ast}),
            expected,
        )?;
        audit(c, ctx, "filter.saved", Some(id), json!({}))?;
        Ok(to_json(d))
    })
}
pub fn validate_rubric(b: &Value) -> Result<(), ApiError> {
    let name = str_field(b, "name")?;
    let criteria = b["criteria"]
        .as_array()
        .ok_or_else(|| ApiError::bad("Добавьте критерии"))?;
    if name.len() > 200 || criteria.is_empty() || criteria.len() > 30 {
        return Err(ApiError::bad("Нужно от 1 до 30 критериев"));
    }
    let mut ids = std::collections::HashSet::new();
    for criterion in criteria {
        let id = str_field(criterion, "id")?;
        let weight = criterion["weight"].as_f64().unwrap_or(0.);
        if !ids.insert(id.to_owned())
            || !weight.is_finite()
            || weight <= 0.
            || weight > 1000.
            || str_field(criterion, "name")?.len() > 200
            || criterion
                .get("instruction")
                .is_some_and(|v| !v.is_string() || v.as_str().unwrap_or("").len() > 4000)
            || criterion.get("required").is_some_and(|v| !v.is_boolean())
            || criterion
                .get("evaluation_mode")
                .is_some_and(|v| !matches!(v.as_str(), Some("score" | "checklist")))
        {
            return Err(ApiError::bad(
                "Проверьте идентификаторы, названия и веса критериев",
            ));
        }
    }
    Ok(())
}
pub fn create_rubric_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    id: Uuid,
    b: Value,
) -> Result<Value, ApiError> {
    validate_rubric(&b)?;
    settings_change(c, ctx, None, |c| {
        if persistence::get_document(c, ctx.organization_id, id)?.is_some() {
            return Err(ApiError::new(409, "already_exists", "Шкала уже существует"));
        }
        let version = persistence::list_documents(c, ctx.organization_id, "rubric")?
            .iter()
            .filter_map(|v| v.data["rubric_version"].as_i64())
            .max()
            .unwrap_or(0)
            + 1;
        let d = persistence::save_document(
            c,
            ctx.organization_id,
            id,
            "rubric",
            None,
            json!({"name":b["name"],"criteria":b["criteria"],"rubric_version":version,"active":false}),
            None,
        )?;
        audit(
            c,
            ctx,
            "rubric.created",
            Some(id),
            json!({"rubric_version":version}),
        )?;
        Ok(to_json(d))
    })
}
pub fn activate_rubric_in_db(
    c: &mut diesel::PgConnection,
    ctx: &auth::Context,
    id: Uuid,
) -> Result<(), ApiError> {
    settings_change(c, ctx, None, |c| {
        document(c, ctx.organization_id, id, "rubric")?;
        for mut d in persistence::list_documents(c, ctx.organization_id, "rubric")? {
            let active = d.id == id;
            if d.data["active"] != active {
                d.data["active"] = json!(active);
                persistence::save_document(
                    c,
                    ctx.organization_id,
                    d.id,
                    "rubric",
                    None,
                    d.data,
                    Some(d.version),
                )?;
            }
        }
        audit(c, ctx, "rubric.activated", Some(id), json!({}))?;
        Ok(())
    })
}

#[cfg(test)]
mod rubric_contract_tests {
    use super::*;
    #[test]
    fn rubric_accepts_legacy_and_explicit_modes_but_rejects_invalid_mode_types() {
        let mut rubric = json!({"name":"Synthetic rubric","criteria":[{"id":"greeting","name":"Greeting","weight":1,"instruction":"Require a greeting","required":true}]});
        validate_rubric(&rubric).unwrap();
        for mode in ["score", "checklist"] {
            rubric["criteria"][0]["evaluation_mode"] = json!(mode);
            validate_rubric(&rubric).unwrap();
        }
        for mode in [json!("invented"), json!(false), Value::Null, json!({})] {
            rubric["criteria"][0]["evaluation_mode"] = mode;
            assert!(validate_rubric(&rubric).is_err());
        }
    }
}
