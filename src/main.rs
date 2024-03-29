use actix_cors::Cors;
use actix_web::{
    dev::ServiceRequest,
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized, Result},
    get,
    middleware::{Condition, Logger},
    post, web, App, HttpServer, Responder,
};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use anyhow::Context;
use chrono::{DateTime, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::{
    migrate,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Sqlite, Transaction,
};
use std::{collections::HashMap, env, str::FromStr};

type DbPool = Pool<Sqlite>;

struct State {
    db: DbPool,
    config: Config,
    token: Option<String>,
}

impl State {
    fn new(pool: DbPool, config: Config, token: Option<String>) -> Self {
        State {
            db: pool,
            config,
            token,
        }
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

async fn init_one(txn: &mut Transaction<'_, Sqlite>, init: &Init) -> Result<()> {
    if let Some(_) = sqlx::query!("SELECT 1 as x FROM color WHERE barcode=?1", init.barcode)
        .fetch_optional(&mut *txn)
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
    .execute(&mut *txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?;

    sqlx::query!(
        "INSERT INTO color (barcode, color) VALUES (?1, ?2)",
        init.barcode,
        init.color
    )
    .execute(&mut *txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?;

    Ok(())
}

#[post("/init")]
async fn init_handler(state: web::Data<State>, init: web::Json<Init>) -> Result<impl Responder> {
    info!("Creating barcode {}", &init.barcode);

    let mut txn = state
        .db
        .begin()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    init_one(&mut txn, &init.0).await?;

    txn.commit()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    Ok("OK")
}

#[post("/init_many")]
async fn init_many_handler(
    state: web::Data<State>,
    inits: web::Json<Vec<Init>>,
) -> Result<impl Responder> {
    let mut txn = state
        .db
        .begin()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    for init in inits.0 {
        info!("Creating barcode {}", &init.barcode);
        init_one(&mut txn, &init).await?;
    }

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

    let current = sqlx::query!(
        r#"SELECT color,station FROM color WHERE barcode=?1"#,
        event.barcode
    )
    .fetch_optional(&mut txn)
    .await
    .map_err(|e| ErrorInternalServerError(e))?
    .ok_or(ErrorNotFound("Barcode does not exist"))?;

    if current.station == Some(event.station) {
        return Err(ErrorBadRequest("Cannot visit same station immediately"));
    }

    let station_def = state
        .config
        .stations
        .get(&event.station)
        .ok_or(ErrorBadRequest("Station invalid"))?;

    let newcolor = match station_def {
        StationDefinition::Function(map) => map.get(&current.color).ok_or(
            ErrorInternalServerError("Bad config: Current color not in map"),
        )?,
        StationDefinition::Cycle(cycle) => {
            let idx: usize = sqlx::query_scalar!(
                r#"SELECT last_index FROM cycle_state WHERE station=?1"#,
                event.station
            )
            .fetch_optional(&mut txn)
            .await
            .map_err(|e| ErrorInternalServerError(e))?
            .unwrap_or(0)
            .try_into()
            .map_err(|e| ErrorInternalServerError(e))?;
            let newcolor = cycle
                .get(idx)
                .ok_or(ErrorInternalServerError("Cycle state out of bounds"))?;
            let idx = ((idx + 1) % cycle.len()) as u32;
            sqlx::query!(r#"INSERT INTO cycle_state (station, last_index) VALUES (?1, ?2) ON CONFLICT(station) DO UPDATE SET last_index=excluded.last_index"#, event.station, idx).execute(&mut txn).await.map_err(|e| ErrorInternalServerError(e))?;
            newcolor
        }
    };

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
        "INSERT INTO color (barcode, station, color) VALUES (?1, ?2, ?3) ON CONFLICT(barcode) DO UPDATE SET color=excluded.color, station=excluded.station",
        event.barcode,
        event.station,
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

#[post("/reset")]
async fn reset_handler(state: web::Data<State>) -> Result<impl Responder> {
    let mut txn = state
        .db
        .begin()
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    sqlx::query!("DELETE FROM event")
        .execute(&mut txn)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    sqlx::query!("DELETE FROM color")
        .execute(&mut txn)
        .await
        .map_err(|e| ErrorInternalServerError(e))?;

    sqlx::query!("DELETE FROM cycle_state")
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
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opt)
        .await?;
    migrate!().run(&pool).await?;
    info!("Database ready");
    Ok(pool)
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum StationDefinition {
    Function(HashMap<String, String>),
    Cycle(Vec<String>),
}

#[derive(Deserialize, Debug)]
struct Config {
    stations: HashMap<i64, StationDefinition>,
}

async fn auth_validator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> std::result::Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    if let Some(state) = req.app_data::<web::Data<State>>() {
        if let Some(required_token) = &state.token {
            if credentials.token() == required_token {
                return Ok(req);
            } else {
                return Err((ErrorUnauthorized("Bad token"), req));
            }
        }
        unreachable!()
    }
    unreachable!()
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let config: Config = serde_json::from_str(
        &std::fs::read_to_string("config.json").context("Can't read config.json")?,
    )?;

    let pool = prepare_db().await?;

    let auth_token = env::var("COLORGAME_AUTH").ok();
    let have_auth = auth_token.is_some();

    if have_auth {
        info!("Authorization configured")
    } else {
        warn!("Running without authorization!")
    }

    let state = web::Data::new(State::new(pool, config, auth_token));
    let state2 = state.clone();
    HttpServer::new(move || {
        let auth = HttpAuthentication::bearer(auth_validator);
        App::new()
            .app_data(state.clone())
            .wrap(Logger::default())
            .wrap(Condition::new(have_auth, auth))
            .wrap(Cors::permissive())
            .service(events_handler)
            .service(current_handler)
            .service(event_handler)
            .service(init_handler)
            .service(init_many_handler)
            .service(at_handler)
            .service(reset_handler)
    })
    .bind(("0.0.0.0", 8000))?
    .run()
    .await?;
    state2.db.close().await;
    info!("Quitting");
    Ok(())
}
