use std::process;

use scel2rime::Config;

fn main() {

    let config = Config::new().unwrap_or_else(|err| {
        eprintln!("Problem parsing arguments: {}", err);
        process::exit(1);
    });

    if let Err(e) = scel2rime::run(config) {
        eprintln!("Application error: {}", e);
        process::exit(1);
    };
}