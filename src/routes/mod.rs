pub mod health;
pub mod verify;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    health::configure(cfg);
    verify::configure(cfg);
}
