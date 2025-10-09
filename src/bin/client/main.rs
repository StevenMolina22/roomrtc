mod client;

use crate::client::Client;
use std::env::args;
use std::io::{BufReader, stdin, stdout};

fn main() {
    let argv = args().collect::<Vec<String>>();
    if argv.len() != 2 {
        eprintln!("Error: wrong number of arguments!");
        return;
    }

    let mut client = Client::new();
    match argv[1].as_str() {
        "0" => {
            if let Err(_) = client.offer_sdp(BufReader::new(stdin()), stdout()) {
                eprintln!();
                return;
            }
        }
        "1" => {
            if let Err(_) = client.answer_sdp(BufReader::new(stdin()), stdout()) {
                eprintln!();
                return;
            }
        }
        _ => eprintln!("Error: wrong client mode"),
    }
}
