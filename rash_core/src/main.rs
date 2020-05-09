//#![deny(warnings)]

#[macro_use]
extern crate lazy_static;

mod constants;
mod context;
mod error;
mod modules;
mod plugins;
#[macro_use]
mod task;

use context::Context;
use plugins::inventory::INVENTORIES;

use std::path::PathBuf;

lazy_static! {
    static ref TASKS_PATH: PathBuf = PathBuf::from("./entrypoint.rh");
}

fn main() {
    let inventory = INVENTORIES.get("env").expect("Inventory does not exists");
    let context = Context::new(TASKS_PATH.to_path_buf(), inventory.load());
    println!("{:?}", context);
}
