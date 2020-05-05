#![deny(warnings)]

mod constants;
mod plugins;

use std::fs;

extern crate yaml_rust;
use yaml_rust::{YamlEmitter, YamlLoader};

const TASKS_PATH: &str = "entrypoint.yml";

fn main() {
    let tasks_file =
        fs::read_to_string(TASKS_PATH.to_string()).expect("Something went wrong reading the file");
    let docs = YamlLoader::load_from_str(&tasks_file).unwrap();

    // Multi document support, doc is a yaml::Yaml
    let doc = &docs[0];

    // Debug support
    println!("{:?}", doc);

    // Index access for map & array
    assert_eq!(doc["foo"][0].as_str().unwrap(), "list1");
    assert_eq!(doc["bar"][1].as_f64().unwrap(), 2.0);

    // Chained key/array access is checked and won't panic,
    // return BadValue if they are not exist.
    assert!(doc["INVALID_KEY"][100].is_badvalue());

    // Dump the YAML object
    let mut out_str = String::new();
    {
        let mut emitter = YamlEmitter::new(&mut out_str);
        emitter.dump(doc).unwrap(); // dump the YAML object to a String
    }
    println!("{}", out_str);
}
