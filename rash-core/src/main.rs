//#![deny(warnings)]

#[macro_use]
extern crate lazy_static;

mod constants;
mod context;
mod executor;
mod modules;
mod plugins;

use context::Context;
use plugins::inventory::env::Inventory;

use std::env;
use std::path::PathBuf;

lazy_static! {
    static ref TASKS_PATH: PathBuf = PathBuf::from("./entrypoint.rh");
}

fn main() {
    let inventory = Inventory::new(env::vars());
    let context = Context::new(TASKS_PATH.to_path_buf(), inventory);
    println!("{:?}", context);
}
