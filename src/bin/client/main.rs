mod client;

use std::env::args;
use std::io::{stdin, stdout, BufReader};
use crate::client::Client;

fn main() {
    let argv = args().collect::<Vec<String>>();
    if argv.len() != 2 {
        eprintln!("Error: wrong number of arguments!");
        return;
    }

    let client = Client::new();
    match argv[1].as_str() {
        "0" => client.offer_sdp(BufReader::new(stdin()), stdout()),
        "1" => client.answer_sdp(BufReader::new(stdin()), stdout()),
        _ => eprintln!("Error: wrong client mode"),
    }
}
