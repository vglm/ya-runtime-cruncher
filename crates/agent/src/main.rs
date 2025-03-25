use base64::prelude::*;
use std::env;
use std::io::Cursor;
use std::time::Duration;

use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[get("/")]
async fn index(_req: HttpRequest) -> impl Responder {
    log::info!("Endpoint: index");
    "Welcome!"
}

#[get("/healthcheck")]
async fn healthcheck(_req: HttpRequest) -> impl Responder {
    log::info!("Endpoint: healthcheck");
    HttpResponse::Ok()
}



#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    log::info!("Dummy runtime. Args: {args:?}");

    Ok(HttpServer::new(|| {
        App::new()
            .service(index)
            .service(healthcheck)
    })
        .bind(("127.0.0.1", 7861))?
        .run()
        .await?)
}