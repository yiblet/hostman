#![feature(proc_macro_hygiene, decl_macro, try_blocks)]

#[macro_use]
extern crate rocket;
extern crate clap;
extern crate serde;
extern crate serde_json;
extern crate sled;
extern crate hostman_shared;

use std::{
    borrow::Borrow,
    sync::{Arc, Mutex, MutexGuard},
};

use clap::{App, Arg};
use hostman_shared::Table;


type Db = Arc<Mutex<sled::Db>>;

#[get("/<hostname>/<ip>")]
fn update(db: rocket::State<Db>, hostname: String, ip: String) -> String {
    db.inner()
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|db: MutexGuard<sled::Db>| {
            let val: &sled::Db = db.borrow();
            let res: sled::Result<_> = try {
                let mut tab = val
                    .get("table")?
                    .and_then(|val: sled::IVec| -> Option<Table> {
                        serde_json::from_slice(val.borrow()).ok()
                    })
                    .unwrap_or_default();
                tab.host_mapping.insert(hostname.clone(), ip.clone());
                let json =
                    serde_json::to_string(&tab).expect("table should always be serializeable");
                val.insert("table", json.as_bytes())
                    .map(|_| ())
                    .unwrap_or_else(|e| eprintln!("{}", e));
                json
            };
            res.map_err(|e| e.to_string())
        })
        .unwrap_or_else(|e| e)
}

#[get("/")]
fn get(db: rocket::State<Db>) -> String {
    db.inner()
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|db: MutexGuard<sled::Db>| {
            let val: &sled::Db = db.borrow();
            let res: sled::Result<_> = try {
                let tab = val
                    .get("table")?
                    .and_then(|val: sled::IVec| -> Option<Table> {
                        serde_json::from_slice(val.borrow()).ok()
                    })
                    .unwrap_or_default();
                let json =
                    serde_json::to_string(&tab).expect("table should always be serializeable");
                val.insert("table", json.as_bytes())
                    .map(|_| ())
                    .unwrap_or_else(|e| eprintln!("{}", e));
                json
            };
            res.map_err(|e| e.to_string())
        })
        .unwrap_or_else(|e| e)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("Hostman Server")
        .version("0.1")
        .author("Shalom Yiblet <shalom.yiblet@gmail.com>")
        .arg(
            Arg::with_name("LOCATION")
                .help("Sets the input file to use")
                .index(1),
        )
        .arg(
            Arg::with_name("port")
                .help("sets the port")
                .long("port")
                .short("p")
                .default_value("15332")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("host")
                .help("sets the host")
                .long("host")
                .short("h")
                .default_value("0.0.0.0")
                .takes_value(true),
        )
        .get_matches();

    let location: &str = matches.value_of("LOCATION").unwrap_or("/tmp/db");

    println!("database at: {}", location);

    let db = Arc::new(Mutex::new(sled::Db::open(location)?));

    let config = rocket::Config::build(rocket::config::Environment::Development)
        .port(
            matches
                .value_of("port")
                .ok_or("port should be available")?
                .parse()?,
        )
        .address(matches.value_of("host").ok_or("host should be available")?)
        .finalize()?;

    rocket::custom(config)
        .manage(db)
        .mount("/update", routes![update])
        .mount("/get", routes![get])
        .launch();

    Ok(())
}
