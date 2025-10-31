use std::env::args;
use std::io::{Read, stdin};

use roomrtc::client::Client;

fn main() {
    let argv = args().collect::<Vec<String>>();
    if argv.len() != 2 {
        eprintln!("Error: wrong number of arguments!");
        return;
    }

    let mut client = Client::new(0);
    match argv[1].as_str() {
        "0" => {
            // Offerer: Print offer, wait for answer
            let offer = client.get_offer();
            println!("{}", offer);

            // Read answer from stdin
            let mut answer_str = String::new();
            if stdin().read_to_string(&mut answer_str).is_err() {
                eprintln!("Error reading answer from stdin");
                return;
            }

            // Process answer
            if let Err(e) = client.process_answer(&answer_str) {
                eprintln!("Error processing answer: {}", e);
            }
        }
        "1" => {
            // Answerer: Wait for offer, print answer
            let mut offer_str = String::new();
            if stdin().read_to_string(&mut offer_str).is_err() {
                eprintln!("Error reading offer from stdin");
                return;
            }

            // Process offer and get answer
            match client.process_offer(&offer_str) {
                Ok(answer_sdp) => {
                    println!("{}", answer_sdp);
                }
                Err(e) => {
                    eprintln!("Error processing offer: {}", e);
                }
            }
        }
        _ => eprintln!("Error: wrong client mode"),
    }
    eprintln!("Handshake complete. Client exiting.");
}
