use std::env;

pub struct Config {
    pub geyser_local_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            geyser_local_url: env::var("GEYSER_LOCAL_URL")
                .expect("GEYSER_LOCAL_URL missing")
        }
    }
}