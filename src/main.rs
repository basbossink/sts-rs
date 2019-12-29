extern crate actix_web;
extern crate chrono;
extern crate serde;

use actix_web::{web, App, HttpResponse, HttpServer, Result};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use std::clone::Clone;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Deserialize, Copy, Debug, Clone)]
struct Datum {
    value: f64,
    time_stamp: i64,
}

struct AppState {
    series: Mutex<HashMap<String, Vec<Datum>>>,
}

async fn get_series(path: web::Path<String>, state: web::Data<AppState>) -> HttpResponse {
    let series_name = path.to_string();
    let series = state.series.lock().unwrap();
    if let Some(serie) = series.get(&series_name) {
        HttpResponse::Ok().content_type("text/plain").body(format!(
            "Series {} has {} values.",
            series_name,
            serie.len()
        ))
    } else {
        HttpResponse::NotFound().body("")
    }
}

async fn add_datum(
    path: web::Path<String>,
    info: web::Json<Datum>,
    state: web::Data<AppState>,
) -> Result<String> {
    let dt = Utc.timestamp(info.time_stamp, 0);
    let series_name = path.to_string();
    let mut w = state.series.lock().unwrap();
    if let Some(series) = w.get_mut(&series_name) {
        series.push(info.0)
    } else {
        let values = vec![info.0];
        w.insert(series_name, values);
    }
    Ok(format!(
        "Administered value {}, for parameter {}, for time {}",
        info.value,
        path,
        dt.format("%Y-%m-%d %H:%M:%S %z")
    ))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let state = web::Data::new(AppState {
        series: Mutex::new(HashMap::new()),
    });
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/{name}", web::get().to(get_series))
            .route("/{name}", web::post().to(add_datum))
    })
    .bind("127.0.0.1:8088")?
    .run()
    .await
}
