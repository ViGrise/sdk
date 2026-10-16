#![allow(dead_code)]

use std::net::SocketAddr;

pub async fn serve(app: axum::Router) -> SocketAddr {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  addr
}
