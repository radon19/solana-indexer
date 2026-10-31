use super::config::Config;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient,};

pub async fn grpc_connect() -> GeyserGrpcClient<> {
    // Extracting url from env
    dotenvy::dotenv().ok();
    let helper = Config::from_env();
    let url = helper.geyser_url;
    let x_token = helper.geyser_api_key;


    let mut tries = 10;

    while tries > 0 {
        println!("Connecting to gRPC...");
        let builder =
            GeyserGrpcClient::build_from_shared(url.clone())
            .expect("Invalid URL")
            .max_decoding_message_size(64 * 1024 * 1024)
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .expect("tls config")
            .x_token(Some(x_token.clone()))
            .expect("x_token");

        match builder.connect().await {
            Ok(client) => {
                println!("Connected");
                return client;
            }
            Err(err) => {
                println!("Connection failed: {err}");
                println!("Retrying in 3 seconds...");

                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }

        tries -= 1;
    }

    panic!("Could not connect after 10 attemps");
}
