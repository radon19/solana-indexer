use super::config::Config;
use yellowstone_grpc_client::{GeyserGrpcClient, Interceptor};

pub async fn grpc_connect() -> GeyserGrpcClient<impl Interceptor> {
    // Extracting url from env
    dotenvy::dotenv().ok();
    let helper = Config::from_env();
    let url = helper.geyser_local_url;

    let mut tries = 10;

    while tries > 0 {
        println!("Connecting to gRPC...");
        let builder =
            GeyserGrpcClient::build_from_shared(url.clone())
            .expect("Invalid URL");

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
