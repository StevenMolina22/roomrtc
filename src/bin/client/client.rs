use std::io::{BufRead, Write};

pub struct Client {
    sdp: String,
}

impl Client {
    pub fn new() -> Self {
        Self {
            sdp: String::from("mi sdp"),
        }
    }

    pub fn offer_sdp<R: BufRead, W: Write>(&self, mut in_buff: R, mut out_buff: W) {
        out_buff.write_all(self.sdp.as_bytes()).unwrap();
        out_buff.write_all("\n".as_bytes()).unwrap();
        out_buff.flush().unwrap();

        let mut answer = String::new();
        in_buff.read_line(&mut answer).unwrap();

        println!("Answer received: {}", answer.trim());
    }

    pub fn answer_sdp<R: BufRead, W: Write>(&self, mut in_buff: R, mut out_buff: W) {
        let mut offer = String::new();
        in_buff.read_line(&mut offer).unwrap();

        println!("Offer received: {}", offer.trim());

        let sdp_answer = "My answer\n";
        out_buff.write_all(sdp_answer.as_bytes()).unwrap();
        out_buff.flush().unwrap();
    }
}
