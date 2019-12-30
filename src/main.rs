extern crate actix;
extern crate actix_web;
extern crate chrono;
extern crate csv;
extern crate dirs;
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
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

const GNUPLOT_COMMANDS: &'static str = r#"set timefmt "%s";
set format x "%Y/%m/%d %H:%M:%S";
set xdata time;
set xtics rotate;
set terminal svg;
set xlabel 'Time';
set key off;
set datafile separator ",";
set autoscale;
set offsets 0.0, 0.0, 0.01, 0.01;
set grid;
set output"#;

#[derive(Deserialize, Serialize, Copy, Debug, Clone)]
struct Datum {
    time_stamp: i64,
    value: f64,
}

struct BackgroundActor {
    data_storage_path: PathBuf,
    image_output_path: PathBuf,
}

impl BackgroundActor {
    pub fn new(data_storage_path: PathBuf, image_output_path: PathBuf) -> BackgroundActor {
        BackgroundActor {
            data_storage_path,
            image_output_path,
        }
    }
}

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

fn append_last_datum(file_name: &PathBuf, data: &Vec<Datum>) {
    let mut options = OpenOptions::new();
    let file = options
        .write(true)
        .create(true)
        .append(true)
        .open(file_name)
        .unwrap();
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);
    if let Some(datum) = data.last() {
        wtr.serialize(datum).unwrap();
    }
    wtr.flush().unwrap();
}

fn generate_plot(series_name: &str, data_file_name: &PathBuf, images_directory: &PathBuf) {
    let full_command = format!(
        r#"{} '{}';
set title '{} over time';
set ylabel '{}';
plot '{}' using 1:2 with lines notitle;"#,
        GNUPLOT_COMMANDS,
        images_directory
            .join(format!("{}.svg", series_name))
            .display(),
        series_name,
        series_name,
        data_file_name.display()
    );
    let output = Command::new("gnuplot")
        .args(&["-e", &full_command])
        .output()
        .expect("failed to execute process");
    println!("Gnuplot command status {}", output.status);
    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
}

impl Handler<WriteCsv> for BackgroundActor {
    type Result = ();
    fn handle(&mut self, msg: WriteCsv, _ctx: &mut Context<Self>) -> Self::Result {
        println!(
            "BackgroundActor received series {} with {} values.",
            msg.series_name,
            msg.data.len()
        );
        let file_name = self
            .data_storage_path
            .join(format!("data_{}.csv", msg.series_name));
        append_last_datum(&file_name, &msg.data);
        generate_plot(&msg.series_name, &file_name, &self.image_output_path);
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

fn env_or_default(key: &str, default: &str) -> String {
    match std::env::var(key) {
        Ok(val) => val,
        _ => default.to_owned(),
    }
}

fn data_dir_or_empty() -> PathBuf {
    match dirs::data_dir() {
        Some(path) => path,
        None => PathBuf::new(),
    }
}
fn ensure_dir(directory: &PathBuf) {
    if !directory.exists() {
        std::fs::create_dir_all(directory.as_path()).unwrap();
    }
}
#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let config_dir = data_dir_or_empty().join(".sts-rs");
    let data_output_path = PathBuf::from(env_or_default(
        "STS_RS_DATA_PATH",
        config_dir.join("data").to_str().unwrap(),
    ));
    let image_output_path = PathBuf::from(env_or_default(
        "STS_RS_IMAGE_PATH",
        config_dir.join("images").to_str().unwrap(),
    ));
    ensure_dir(&data_output_path);
    ensure_dir(&image_output_path);
    println!("Using data directory {}", data_output_path.display());
    println!("Using image directory {}", image_output_path.display());

    let bt_actor = BackgroundActor::new(data_output_path, image_output_path).start();
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
