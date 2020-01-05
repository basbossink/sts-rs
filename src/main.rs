extern crate actix;
extern crate actix_web;
extern crate chrono;
extern crate csv;
extern crate dirs;
extern crate serde;

#[macro_use]
extern crate log;

use actix::prelude::*;
use actix_files as fs;
use actix_web::{middleware, web, App, HttpResponse, HttpServer, Result};
use askama::Template;
use chrono::{DateTime, TimeZone, Utc};
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use serde::Deserialize;
use serde::Serialize;
use std::clone::Clone;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str;
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

struct SeriesInfo<'a> {
    name: &'a str,
    last_modified: String,
    number_of_observations: usize,
}

#[derive(Template)]
#[template(path = "index.html")]
struct AvailableSeries<'a> {
    series: Vec<SeriesInfo<'a>>,
}

#[derive(Deserialize, Serialize, Copy, Debug, Clone)]
#[allow(non_snake_case)]
struct Datum {
    timeStamp: i64,
    value: f64,
}

struct Series {
    data: Vec<Datum>,
    last_modification_time: DateTime<Utc>,
}

struct AppState {
    background_actor: Addr<BackgroundActor>,
    series: Mutex<HashMap<String, Series>>,
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
    log_command_failure(&output);
}

fn log_command_failure(output: &Output) {
    if !output.status.success() {
        warn!("Gnuplot command failed with status code: {}", output.status);
    }
    if output.stdout.len() > 0 {
        info!(
            "Gnuplot command ouput: {}\n",
            str::from_utf8(&output.stdout).unwrap()
        );
    }
    if output.stderr.len() > 0 {
        warn!(
            "Gnuplot command stderr:\n{}",
            str::from_utf8(&output.stderr).unwrap()
        );
    }
}

impl Handler<WriteCsv> for BackgroundActor {
    type Result = ();
    fn handle(&mut self, msg: WriteCsv, _ctx: &mut Context<Self>) -> Self::Result {
        info!(
            "BackgroundActor received series {} with {} values.",
            msg.series_name,
            msg.data.len()
        );
        let file_name = self
            .data_storage_path
            .join(format!("{}.csv", msg.series_name));
        append_last_datum(&file_name, &msg.data);
        generate_plot(&msg.series_name, &file_name, &self.image_output_path);
    }
}

async fn index(state: web::Data<AppState>) -> Result<HttpResponse> {
    let series = state.series.lock().unwrap();
    let mut infos = series
        .iter()
        .map(|(key, val)| SeriesInfo {
            name: key,
            number_of_observations: val.data.len(),
            last_modified: format!("{}", val.last_modification_time.format("%+")),
        })
        .into_iter()
        .collect::<Vec<_>>();
    infos.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    let rendered = AvailableSeries { series: infos }.render().unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(rendered))
}

async fn get_series(path: web::Path<String>, state: web::Data<AppState>) -> HttpResponse {
    let series_name = path.to_string();
    let series = state.series.lock().unwrap();
    if let Some(serie) = series.get(&series_name) {
        HttpResponse::Ok().content_type("text/plain").body(format!(
            "Series {} has {} values.",
            series_name,
            serie.data.len()
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
    let dt = Utc.timestamp(info.timeStamp, 0);
    let series_name = path.to_string();
    let mut w = state.series.lock().unwrap();
    let current_values = if let Some(series) = w.get_mut(&series_name) {
        series.data.push(info.0);
        series.data.to_vec()
    } else {
        let values = vec![info.0];
        w.insert(
            series_name.clone(),
            Series {
                data: values.to_vec(),
                last_modification_time: Utc::now(),
            },
        );
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

fn read_series(data_output_path: &PathBuf) -> HashMap<String, Series> {
    let mut result: HashMap<String, Series> = HashMap::new();
    for file in data_output_path.read_dir().expect("read_dir call failed") {
        if let Ok(entry) = file {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    info!("Reading data from {:?}", entry.path());
                    let file_path = entry.path();
                    let series_name = file_path.file_stem().unwrap();
                    let (data, last_modified) = read_csv_data(&file_path);
                    let dt = Utc.timestamp(last_modified, 0);
                    let number_of_data_items = data.len();
                    result.insert(
                        series_name.to_os_string().into_string().unwrap(),
                        Series {
                            data,
                            last_modification_time: dt,
                        },
                    );
                    info!(
                        "Finished reading {} values from {:?}",
                        number_of_data_items,
                        entry.path()
                    );
                }
            }
        }
    }
    result
}

fn read_csv_data(file_path: &Path) -> (Vec<Datum>, i64) {
    let mut last_modified = std::i64::MIN;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(&file_path)
        .unwrap();
    let data: Vec<Datum> = rdr
        .records()
        .map(|result| {
            let record = result.unwrap();
            let time_stamp = record.get(0).unwrap().parse::<i64>().unwrap();
            let value = record.get(1).unwrap().parse::<f64>().unwrap();
            if last_modified < time_stamp {
                last_modified = time_stamp;
            }
            Datum {
                timeStamp: time_stamp,
                value,
            }
        })
        .collect();
    (data, last_modified)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
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
    info!("Using data directory {}", data_output_path.display());
    info!("Using image directory {}", image_output_path.display());
    let series = read_series(&data_output_path);
    let bt_actor = BackgroundActor::new(
        data_output_path.to_path_buf(),
        image_output_path.to_path_buf(),
    )
    .start();
    let state = web::Data::new(AppState {
        background_actor: bt_actor.clone(),
        series: Mutex::new(series),
    });

    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder
        .set_private_key_file("key.pem", SslFiletype::PEM)
        .unwrap();
    builder.set_certificate_chain_file("cert.pem").unwrap();
    let url = "127.0.0.1:8443";
    info!("Listening on {}.", url);
    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .service(fs::Files::new(
                "/images",
                image_output_path.to_str().unwrap(),
            ))
            .service(fs::Files::new("/static", "static"))
            .service(fs::Files::new("/favicon.ico", "static/favicon.ico"))
            .app_data(state.clone())
            .route("/", web::get().to(index))
            .route("/{name}", web::get().to(get_series))
            .route("/{name}", web::post().to(add_datum))
    })
    .bind_openssl(url, builder)?
    .run()
    .await
}
