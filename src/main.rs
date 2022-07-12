use actix_cors::Cors;
use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound, Result},
    get,
    middleware::Logger,
    post, web, App, HttpServer, Responder,
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use sqlx::{migrate, sqlite::SqliteConnectOptions, Pool, Sqlite, SqlitePool};
use std::{collections::HashMap, str::FromStr};

type DbPool = Pool<Sqlite>;

struct State {
    db: DbPool,
    config: Config,
}

impl State {
    fn new(pool: DbPool, config: Config) -> Self {
        State { db: pool, config }
    }
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct Event {
    barcode: String,
    station: Option<i64>,
    timestamp: DateTime<Utc>,
    newcolor: String,
}

#[derive(Deserialize)]
struct NewEvent {
    barcode: String,
    station: i64,
}

#[derive(Deserialize)]
struct Init {
    barcode: String,
    color: String,
    timestamp: DateTime<Utc>,
}

#[get("/events")]
async fn events_handler(state: web::Data<State>) -> Result<impl Responder> {
    let events: Vec<Event> = sqlx::query_as("SELECT * FROM event")
        .fetch_all(&state.db)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    Ok(web::Json(events))
}

#[get("/current")]
async fn current_handler(state: web::Data<State>) -> Result<impl Responder> {
    let evmap: HashMap<_, _> = sqlx::query!("SELECT barcode,color FROM color")
        .fetch_all(&state.db)
        .await
        .map_err(|e| ErrorInternalServerError(e))?
        .into_iter()
        .map(|r| (r.barcode, r.color))
        .collect();
    Ok(web::Json(evmap))
}

#[get("/at/{ts}")]
async fn at_handler(
    state: web::Data<State>,
    path: web::Path<(DateTime<Utc>,)>,
) -> Result<impl Responder> {
    let evmap: HashMap<_, _> = sqlx::query_file!("sql/at.sql", path.0)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ErrorInternalServerError(e))?
        .into_iter()
        .map(|r| (r.barcode, r.color))
        .collect();
    Ok(web::Json(evmap))
}

#[post("/init")]
async fn init_handler(state: web::Data<State>, init: web::Json<Init>) -> Result<impl Responder> {
    info!("Creating barcode {}", &init.barcode);

    let mut txn = state
        .db
        .begin()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    if let Some(_) = sqlx::query!("SELECT 1 as x FROM color WHERE barcode=?1", init.barcode)
        .fetch_optional(&mut txn)
        .await
        .map_err(|e| ErrorInternalServerError(e))?
    {
        return Err(ErrorBadRequest("Already exists"));
    }

    sqlx::query!(
        "INSERT INTO event (barcode, timestamp, newcolor) VALUES (?1, ?2, ?3)",
        init.barcode,
        init.timestamp,
        init.color
    )
    .execute(&mut txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?;

    sqlx::query!(
        "INSERT INTO color (barcode, color) VALUES (?1, ?2)",
        init.barcode,
        init.color
    )
    .execute(&mut txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?;

    txn.commit()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    Ok("OK")
}

#[post("/event")]
async fn event_handler(
    state: web::Data<State>,
    event: web::Json<NewEvent>,
) -> Result<impl Responder> {
    let mut txn = state
        .db
        .begin()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    let current = sqlx::query_scalar!(r#"SELECT color FROM color WHERE barcode=?1"#, event.barcode)
        .fetch_optional(&mut txn)
        .await
        .map_err(|e| ErrorInternalServerError(e))?
        .ok_or(ErrorNotFound("Barcode does not exist"))?;

    let color_map = state
        .config
        .stations
        .get(&event.station.to_string())
        .ok_or(ErrorBadRequest("Station invalid"))?;

    let newcolor = color_map
        .get(&current)
        .ok_or(ErrorInternalServerError("Current color not in map"))?;
    // let newcolor = eval
    //     .borrow_mut()
    //     .eval(current, expression)
    //     .map_err(|e| ErrorInternalServerError(format!("Failed to compute new color: {}", e)))?;

    let now = Utc::now();
    sqlx::query!(
        "INSERT INTO event (barcode, station, timestamp, newcolor) VALUES (?1, ?2, ?3, ?4)",
        event.barcode,
        event.station,
        now,
        newcolor
    )
    .execute(&mut txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?;

    sqlx::query!(
        "INSERT INTO color (barcode, color) VALUES (?1, ?2) ON CONFLICT(barcode) DO UPDATE SET color=excluded.color",
        event.barcode,
        newcolor,
    )
    .execute(&mut txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?;

    txn.commit()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;
    Ok("OK")
}

async fn prepare_db() -> anyhow::Result<DbPool> {
    let dbpath = std::env::var("DATABASE_URL").context("Env DATABASE_URL is not set")?;
    let opt = SqliteConnectOptions::from_str(&dbpath)?.create_if_missing(true);
    let pool = SqlitePool::connect_with(opt).await?;
    migrate!().run(&pool).await?;
    info!("Database ready");
    Ok(pool)
}

#[derive(Deserialize, Debug)]
struct Config {
    stations: HashMap<String, HashMap<String, String>>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv()?;
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let config: Config = serde_json::from_str(
        &std::fs::read_to_string("config.json").context("Can't read config.json")?,
    )?;

    let pool = prepare_db().await?;
    let state = web::Data::new(State::new(pool, config));
    let state2 = state.clone();
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(Cors::default().allow_any_origin())
            .app_data(state.clone())
            .service(events_handler)
            .service(current_handler)
            .service(event_handler)
            .service(init_handler)
            .service(at_handler)
    })
    .bind(("127.0.0.1", 8000))?
    .run()
    .await?;
    state2.db.close().await;
    info!("Quitting");
    Ok(())
}
