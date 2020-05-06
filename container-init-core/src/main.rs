#![deny(warnings)]

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

const TASKS_PATH: &str = "entrypoint.yml";

fn main() {
    let inventory = Inventory::new(env::vars());
    let context = Context::new(TASKS_PATH, inventory);
    println!("{:?}", context);
}
