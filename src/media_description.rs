use std::fmt::{Display};
use std::str::FromStr;

const RTPMAP_KEY: &str = "rtpmap";
pub struct MediaDescription {
    media_type: String,
    port: u16,
    protocol: String,
    fmts: Vec<usize>,
    attributes: Vec<String>,
}

impl FromStr for MediaDescription {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_vec: Vec<&str> = s.split_whitespace().collect();
        if s_vec.len() < 4 {
            return Err(());
        }

        let port = s_vec[1].parse::<u16>().map_err(|_| ())?;
        
        let mut parsed_fmt = Vec::new();
        for f_string in &s_vec[3..] {
            let fmt = f_string.parse::<usize>().map_err(|_| ())?;
            parsed_fmt.push(fmt);
        }

        Ok(Self::new(s_vec[0].into(), port, s_vec[2].into(), parsed_fmt))
    }
}

impl Display for MediaDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut content = format!("m={} {} {}", self.media_type, self.port, self.protocol);
        for fmt in &self.fmts {
            content = format!("{} {}", content, fmt)
        }

        for attr in &self.attributes {
            content = format!("{}\na={}", content, attr)
        }

        write!(f, "{}", content)
    }
}

impl MediaDescription {
    pub fn new(media_type: String, port: u16, protocol: String, fmts: Vec<usize>) -> Self {
        Self {
            media_type,
            port,
            protocol,
            fmts,
            attributes: Vec::new(),
        }
    }
    
    pub fn add_attribute(&mut self, attribute_type: String, attribute_body: String) -> Result<(), String>{
        if attribute_type == RTPMAP_KEY {
            match attribute_body.split_whitespace().collect::<Vec<&str>>()[..] {
                [f, _] => {
                    let f = f.parse::<usize>().map_err(|_| "Error parsing attribute body")?;
                    if !self.valid_fmt(f) {
                        return Err("Error parsing attribute body".to_string());
                    }
                },
                _ => return Err("Error splitting attribute body".to_string()),
            };
        }

        self.attributes.push(format!("{}:{}", attribute_type, attribute_body));
        Ok(())
    }

    fn valid_fmt(&self, fmt: usize) -> bool {
        for f in &self.fmts {
            if *f == fmt {
                return true
            }
        }
        false
    }
}