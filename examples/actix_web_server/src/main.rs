use actix_web::{web, App, HttpServer};
use facet::Facet;

#[derive(Debug, Facet)]
struct Query2 {
    #[facet(default = Range { min: 2, max: 4 })]
    range: Range,
}

#[derive(Debug, Facet)]
#[facet(deny_unknown_fields)]
struct Query {
    name: String,

    number: Option<i32>,

    // you can put expression in the default values
    #[facet(default = Range { min: 2, max: 4 })]
    range: Range,

    #[facet(rename = "return")]
    returns: Return,
}

#[derive(Debug, Facet)]
#[facet(invariants = validate_range)]
struct Range {
    min: u8,
    max: u8,
}

fn validate_range(range: &Range) -> bool {
    range.min <= range.max
}

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
#[repr(C)]
enum Return {
    Name,
    Number,
}

/// This handler uses the official `actix_web` `serde_json` extractor
async fn facet(item: facet_actix::Json<Query>) -> facet_actix::Json<Query> {
    item
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    log::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(|| App::new().service(web::resource("/").route(web::post().to(facet))))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
