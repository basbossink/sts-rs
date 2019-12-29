extern crate actix;
extern crate actix_web;
extern crate chrono;
extern crate csv;
extern crate serde;

use actix::prelude::*;
use actix_web::{web, App, HttpResponse, HttpServer, Result};
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use serde::Serialize;
use std::clone::Clone;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::process::Command;
use std::sync::Mutex;

const GNUPLOT_COMMANDS: &'static str = r#"set timefmt "%s";
set format x "%Y/%m/%d %H:%M:%S";
set xdata time;
set xtics rotate;
set terminal svg;
set xlabel 'Time';
set output"#;

#[derive(Deserialize, Serialize, Copy, Debug, Clone)]
struct Datum {
    time_stamp: i64,
    value: f64,
}

#[derive(Default)]
struct BackgroundActor;

struct WriteCsv {
    series_name: String,
    data: Vec<Datum>,
}

impl Message for WriteCsv {
    type Result = ();
}

impl Actor for BackgroundActor {
    type Context = Context<Self>;
}

impl Handler<WriteCsv> for BackgroundActor {
    type Result = ();
    fn handle(&mut self, msg: WriteCsv, _ctx: &mut Context<Self>) -> Self::Result {
        println!(
            "BackgroundActor received series {} with {} values.",
            msg.series_name,
            msg.data.len()
        );
        let file_name = format!("data_{}.csv", msg.series_name);
        let mut options = OpenOptions::new();
        let file = options
            .write(true)
            .create(true)
            .append(true)
            .open(&file_name)
            .unwrap();
        let mut wtr = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(file);
        if let Some(datum) = msg.data.last() {
            wtr.serialize(datum).unwrap();
        }
        wtr.flush().unwrap();
        let full_command = format!(
            r#"{} 'plot_{}_{}.svg';
set title '{} over time';
set ylabel '{}';
plot '{}' using 1:0 with lines"#,
            GNUPLOT_COMMANDS,
            msg.series_name,
            msg.data.len(),
            msg.series_name,
            msg.series_name,
            &file_name
        );
        let output = Command::new("gnuplot")
            .args(&["-e", &full_command])
            .output()
            .expect("failed to execute process");
        println!("Gnuplot command status {}", output.status);
        io::stdout().write_all(&output.stdout).unwrap();
        io::stderr().write_all(&output.stderr).unwrap();
    }
}

struct AppState {
    background_actor: Addr<BackgroundActor>,
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
    let current_values = if let Some(series) = w.get_mut(&series_name) {
        series.push(info.0);
        series.to_vec()
    } else {
        let values = vec![info.0];
        w.insert(series_name.clone(), values.to_vec());
        values
    };
    state.background_actor.do_send(WriteCsv {
        series_name,
        data: current_values,
    });

    Ok(format!(
        "Administered value {}, for parameter {}, for time {}",
        info.value,
        path,
        dt.format("%Y-%m-%d %H:%M:%S %z")
    ))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let bt_actor = BackgroundActor {}.start();
    let state = web::Data::new(AppState {
        background_actor: bt_actor.clone(),
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
