use actix_web::{middleware::DefaultHeaders, web, App, HttpResponse, HttpServer};
use diesel::{prelude::*, sql_types::Integer};
use phone_api::{ok, ApiError, AppState};
use serde_json::json;
#[derive(QueryableByName)]
struct One {
    #[diesel(sql_type=Integer)]
    value: i32,
}
async fn health(state: web::Data<AppState>) -> HttpResponse {
    ok(json!({"status":"ok","demo_mode":state.demo,"service":"phone-api"}))
}
async fn ready(state: web::Data<AppState>) -> Result<HttpResponse, ApiError> {
    state
        .db(|c| {
            let _ = diesel::sql_query("SELECT 1::integer AS value")
                .get_result::<One>(c)?
                .value;
            Ok(())
        })
        .await?;
    Ok(ok(json!({"status":"ready","database":"ready"})))
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        let result = reqwest::Client::new()
            .get("http://127.0.0.1:8080/ready")
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await;
        std::process::exit(if result.is_ok_and(|r| r.status().is_success()) {
            0
        } else {
            1
        });
    }
    let run = async {
        let url =
            std::env::var("DATABASE_URL").map_err(|_| ApiError::bad("DATABASE_URL обязателен"))?;
        let pool = persistence::create_pool(&url)?;
        persistence::migrate(&pool)?;
        let state = AppState::from_env(pool)?;
        if std::env::args().nth(1).as_deref() == Some("bootstrap") {
            let email = std::env::var("BOOTSTRAP_EMAIL")
                .map_err(|_| ApiError::bad("BOOTSTRAP_EMAIL обязателен"))?;
            let password = std::env::var("BOOTSTRAP_PASSWORD")
                .map_err(|_| ApiError::bad("BOOTSTRAP_PASSWORD обязателен"))?;
            let name = std::env::var("BOOTSTRAP_NAME").unwrap_or_else(|_| "Владелец".into());
            let org = std::env::var("BOOTSTRAP_ORGANIZATION")
                .map_err(|_| ApiError::bad("BOOTSTRAP_ORGANIZATION обязателен"))?;
            state
                .db(move |c| {
                    phone_api::auth::bootstrap(c, &email, &password, &name, &org)?;
                    Ok(())
                })
                .await?;
            println!("bootstrap_completed");
            return Ok::<(), ApiError>(());
        }
        phone_api::seed::seed_demo(&state).await?;
        let maintenance_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                if phone_api::auth::registration_maintenance(&maintenance_state).await.is_err() {
                    eprintln!("registration_maintenance_failed");
                }
                if phone_api::assistant::maintenance(&maintenance_state)
                    .await
                    .is_err()
                {
                    eprintln!("assistant_maintenance_failed");
                }
            }
        });
        let bind = std::env::var("API_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
        println!("phone_api_starting");
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(state.clone()))
                .app_data(
                    web::JsonConfig::default()
                        .limit(2 * 1024 * 1024)
                        .error_handler(|_, _| ApiError::bad("Неверный JSON запроса").into()),
                )
                .app_data(
                    web::PathConfig::default()
                        .error_handler(|_, _| ApiError::bad("Неверный идентификатор").into()),
                )
                .app_data(
                    web::QueryConfig::default()
                        .error_handler(|_, _| ApiError::bad("Неверные параметры запроса").into()),
                )
                .wrap(
                    DefaultHeaders::new()
                        .add(("X-Content-Type-Options", "nosniff"))
                        .add(("X-Frame-Options", "DENY"))
                        .add(("Referrer-Policy", "same-origin"))
                        .add(("Cache-Control", "no-store")),
                )
                .route("/health", web::get().to(health))
                .route("/ready", web::get().to(ready))
                .configure(phone_api::configure)
                .default_service(web::route().to(|| async {
                    HttpResponse::NotFound()
                        .json(json!({"error":{"code":"not_found","message":"Маршрут не найден"}}))
                }))
        })
        .workers(4)
        .bind(bind)
        .map_err(|_| ApiError::new(500, "bind_failed", "Порт сервера недоступен"))?
        .run()
        .await
        .map_err(|_| ApiError::internal())?;
        Ok(())
    }
    .await;
    if let Err(error) = run {
        eprintln!("{}", error.code);
        return Err(std::io::Error::other(error.code));
    }
    Ok(())
}
