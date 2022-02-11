use anyhow::Result;
use audit_node::NodeUpdate;
use axum::{
  extract::{
    ws::{Message, WebSocketUpgrade},
    Extension, TypedHeader,
  },
  http::StatusCode,
  response::IntoResponse,
  routing::{get, get_service},
  AddExtensionLayer, Router,
};
use serde_json::json;
use std::{intrinsics::transmute, mem::size_of, net::SocketAddr};
use tokio::{net::UdpSocket, sync::watch};
use tower_http::{
  services::ServeDir,
  trace::{DefaultMakeSpan, TraceLayer},
};
use tracing::{error, info, trace, Level};

async fn start_ev_listener(publisher: watch::Sender<NodeUpdate>) -> Result<()> {
  let addr = SocketAddr::from(([0, 0, 0, 0], 9000));
  info!("starting audit listener at {}", addr);
  let socket = UdpSocket::bind(addr).await?;
  let mut buffer = [0u8; size_of::<NodeUpdate>()];

  loop {
    let (addr, len) = socket.recv_from(&mut buffer).await?;
    let update: NodeUpdate = unsafe { transmute(buffer) };
    trace!(
      "Received Node update from ({} | {}): {:?}",
      addr,
      len,
      update
    );
    publisher.send(update)?;
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG)
    .init();
  let (tx, rx) = watch::channel(Default::default());
  let app = Router::new()
    .fallback(
      get_service(
        ServeDir::new("assets").append_index_html_on_directories(true),
      )
      .handle_error(|error: std::io::Error| async move {
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          format!("Unhandled internal error: {}", error),
        )
      }),
    )
    .route("/stream", get(ws_handler))
    .layer(AddExtensionLayer::new(rx))
    .layer(
      TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::default().include_headers(true)),
    );

  let addr = SocketAddr::from(([0, 0, 0, 0], 80));
  info!("Audit node UI listening on {}", addr);

  tokio::spawn(async move {
    if let Err(e) = start_ev_listener(tx).await {
      error!("Error in event listener task: {:?}", e);
    }
  });

  axum::Server::bind(&addr)
    .serve(app.into_make_service())
    .await
    .unwrap();
  Ok(())
}

async fn ws_handler(
  ws: WebSocketUpgrade,
  Extension(subscriber): Extension<watch::Receiver<NodeUpdate>>,
  user_agent: Option<TypedHeader<headers::UserAgent>>,
) -> impl IntoResponse {
  if let Some(TypedHeader(user_agent)) = user_agent {
    info!("`{}` connected", user_agent.as_str());
  }
  ws.on_upgrade(|mut socket| async move {
    info!("handling websocket request");
    let mut subscriber = subscriber.clone();
    loop {
      while subscriber.changed().await.is_ok() {
        let val = *subscriber.borrow();
        trace!("websocket stream value changed: {:?}", val);

        #[allow(unaligned_references)]
        if let Err(err) = socket
          .send(Message::Text(
            serde_json::to_string(&json!({
              "node_id": val.node_id.to_base58(),
              "peer_id": val.peer_id.to_base58(),
              "event": val.event
            }))
            .unwrap(),
          ))
          .await
        {
          error!("failed to publish message: {:?}", err);
          return;
        }
      }
    }
  })
}
