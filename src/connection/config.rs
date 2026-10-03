use std::env;

pub struct Config {
    pub geyser_local_url: String,
    pub geyser_url: String,
    pub geyser_api_key:String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            geyser_local_url: env::var("GEYSER_LOCAL_URL")
                .expect("GEYSER_LOCAL_URL missing"),
            geyser_url: env::var("GEYSER_URL")
                .expect("GEYSER_URL missing"),
            geyser_api_key: env::var("GEYSER_X_TOKEN")
                .expect("API KEY NOT FOUND IN ENV"),
        }
    }
}