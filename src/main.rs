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
use std::cell::RefCell;
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
    newcolor: u32,
}

#[derive(Deserialize)]
struct NewEvent {
    barcode: String,
    station: i64,
}

#[derive(Deserialize)]
struct Init {
    barcode: String,
    color: u32,
    timestamp: DateTime<Utc>,
}

mod expr {
    use anyhow::{anyhow, Result};
    use rhai::{
        packages::{CorePackage, Package},
        Engine, Scope, INT,
    };

    #[derive(Clone, Debug)]
    struct Color {
        r: u8,
        g: u8,
        b: u8,
    }

    impl Color {
        fn new(r: INT, g: INT, b: INT) -> Self {
            Self {
                r: r as u8,
                g: g as u8,
                b: b as u8,
            }
        }
    }

    pub struct Eval {
        engine: Engine,
    }

    impl Eval {
        pub fn new() -> Self {
            let mut engine = Engine::new_raw();
            let package = CorePackage::new();
            engine.register_global_module(package.as_shared_module());
            engine
                .register_type::<Color>()
                .register_fn("rgb", Color::new);
            Self { engine }
        }

        pub fn eval(&mut self, color: u32, expr: &str) -> Result<u32> {
            let mut scope = Scope::new();
            scope.push("r", (color >> 16 & 0xff) as i64);
            scope.push("g", (color >> 8 & 0xff) as i64);
            scope.push("b", (color & 0xff) as i64);
            let color: Color = self
                .engine
                .eval_expression_with_scope(&mut scope, expr)
                .map_err(|e| anyhow!(format!("{}", e)))?;
            Ok(((color.r as u32) << 16) + ((color.g as u32) << 8) + (color.b as u32))
        }
    }
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
    eval: web::Data<RefCell<expr::Eval>>,
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
    let current = current as u32;

    let expression = state
        .config
        .stations
        .get(&event.station.to_string())
        .ok_or(ErrorBadRequest("Station invalid"))?;

    let newcolor = eval
        .borrow_mut()
        .eval(current, expression)
        .map_err(|e| ErrorInternalServerError(format!("Failed to compute new color: {}", e)))?;

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
    let opt = SqliteConnectOptions::from_str("sqlite://data.db")?.create_if_missing(true);
    let pool = SqlitePool::connect_with(opt).await?;
    migrate!().run(&pool).await?;
    info!("Database ready");
    Ok(pool)
}

#[derive(Deserialize, Debug)]
struct Config {
    stations: HashMap<String, String>,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let config: Config =
        toml::from_str(&std::fs::read_to_string("config.toml").context("Can't read config.toml")?)?;

    let pool = prepare_db().await?;
    let state = web::Data::new(State::new(pool, config));
    Ok(HttpServer::new(move || {
        let eval = expr::Eval::new();
        App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .app_data(web::Data::new(RefCell::new(eval)))
            .service(events_handler)
            .service(current_handler)
            .service(event_handler)
            .service(init_handler)
            .service(at_handler)
    })
    .bind(("127.0.0.1", 8000))?
    .run()
    .await?)
}
