use axum::Router;
use wobblescope::web;

const LISTEN_ADDR: &str = "127.0.0.1:7126";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app = Router::new().fallback(web::static_handler);
    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR).await?;
    axum::serve(listener, app).await
}
