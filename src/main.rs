mod datastore;
mod error;
mod model;
mod payment_service;

use crate::datastore::PickleDatastore;

use crate::payment_service::PaymentService;
use clap::{App, Arg};

#[macro_use]
extern crate derive_more;
#[macro_use]
extern crate log;
#[macro_use]
extern crate clap;

const CSV_INPUT_FILE: &str = "CSV_INPUT_FILE";

fn main() {
    let arg_matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .arg(
            Arg::with_name(CSV_INPUT_FILE)
                .help("Path for the CSV input file")
                .required(true)
                .index(1),
        )
        .get_matches();
    let csv_path = arg_matches
        .value_of(CSV_INPUT_FILE)
        .expect("CSV input file path is expected for app to run");

    env_logger::init();

    info!("Starting transaction processing");

    let datastore = PickleDatastore::new();
    let mut service = PaymentService::new(Box::new(datastore));

    match service.run(&*csv_path) {
        Ok(_) => {
            info!("Processed all transactions");
        }
        Err(e) => {
            error!("Fatal {}", e);
        }
    }
}
