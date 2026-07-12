mod app;
mod auth;
mod headless;
mod input;
mod model;
mod prelude;
mod render;
mod runtime;
mod storage;
mod ui;
mod worker;

pub(crate) use app::*;
pub(crate) use auth::*;
pub(crate) use input::*;
pub(crate) use model::*;
pub(crate) use render::*;
pub(crate) use storage::*;
pub(crate) use ui::*;
pub(crate) use worker::*;

fn main() -> model::AnyResult<()> {
    runtime::main_entry()
}
