//!
//! P2P Protocols Lab
//!
//! This executable is the entrypoint to running the development version of libp2p-episub.
//! Using this executable you can crate and test the behavior of p2p gossip networks.
//!
//! At some point when episub implementation reaches a certain level of maturity it should
//! be packaged into a separate crate that could be used as a module in libp2p. However for
//! now, while we are still very early in the development lifecycle, its more convenient to have
//! everything in one place.
//!

use std::{intrinsics::transmute, mem::size_of, time::Duration};

use anyhow::Result;
use audit_node::{NodeEvent, NodeUpdate};
use futures::StreamExt;
use libp2p::{identity, swarm::SwarmEvent, Multiaddr, PeerId};
use libp2p_episub::{Config, Episub, EpisubEvent};
use structopt::StructOpt;
use tokio::{net::UdpSocket, sync::mpsc::unbounded_channel};
use tracing::{error, info, trace, Level};

static DEFAULT_BOOTSTRAP_NODE: &str = "/dnsaddr/bootstrap.libp2p.io";

#[derive(Debug, StructOpt)]
struct CliOptions {
  #[structopt(short, long, about = "gossip topic name")]
  topic: Vec<String>,

  #[structopt(long, default_value=DEFAULT_BOOTSTRAP_NODE, about = "p2p bootstrap peers")]
  bootstrap: Vec<Multiaddr>,

  #[structopt(long, about = "p2p audit node")]
  audit: String,

  #[structopt(long, default_value = "100", about = "p2p audit node")]
  size: usize,

  #[structopt(long, about = "if this node publishes messages")]
  sender: bool,
}

async fn send_update(addr: &str, update: NodeUpdate) -> Result<()> {
  let buf: [u8; size_of::<NodeUpdate>()] = unsafe { transmute(update) };
  let audit_sock = UdpSocket::bind("0.0.0.0:9000").await?;
  audit_sock.connect(addr).await?;
  audit_sock.send(&buf).await.unwrap_or_else(|err| {
    error!("failed to update audit node: {:?}", err);
    0
  });
  Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
  let opts = CliOptions::from_args();

  tracing_subscriber::fmt::init();

  let local_key = identity::Keypair::generate_ed25519();
  let local_peer_id = PeerId::from(local_key.public());

  info!("Local peer id: {:?}", local_peer_id);
  info!("Bootstrap nodes: {:?}", opts.bootstrap);
  info!("Gossip Topics: {:?}", opts.topic);

  // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
  let transport = libp2p::development_transport(local_key.clone()).await?;

  info!("audit addr: {:?}", opts.audit);

  send_update(
    &opts.audit,
    NodeUpdate {
      node_id: local_peer_id,
      peer_id: local_peer_id,
      event: NodeEvent::Up,
    },
  )
  .await
  .unwrap_or_else(|err| {
    error!("Failed notifying audit node about node startup: {:?}", err);
  });

  // Create a Swarm to manage peers and events
  let mut swarm = libp2p::Swarm::new(
    transport,
    Episub::new(Config {
      network_size: opts.size,
      ..Config::default()
    }),
    local_peer_id,
  );

  // Listen on all interfaces and whatever port the OS assigns
  swarm
    .listen_on("/ip4/0.0.0.0/tcp/4001".parse().unwrap())
    .unwrap();

  // subscribe to the topic specified on the command line
  opts.topic.iter().for_each(|t| {
    swarm.behaviour_mut().subscribe(t.clone());
  });

  // dial all bootstrap nodes
  opts
    .bootstrap
    .into_iter()
    .for_each(|addr| swarm.dial(addr).unwrap());

  let (msg_tx, mut msg_rx) = unbounded_channel::<Vec<u8>>();
  tokio::spawn(async move {
    if opts.sender {
      loop {
        // every 5 seconds send a message to the gossip topic
        tokio::time::sleep(Duration::from_secs(1)).await;
        msg_tx
          .send(
            [1u8, 2, 3, 4, 5]
              .into_iter()
              .cycle()
              .take(1024 * 800)
              .collect(),
          )
          .unwrap_or_else(|err| {
            error!("periodic message thread error: {:?}", err);
          })
      }
    }
  });

  let mut i = 0;
  // run the libp2p event loop in conjunction with our send loop
  loop {
    tokio::select! {
      Some(event) = swarm.next() => {
        match event {
          SwarmEvent::Behaviour(b) => {
            match b {
              EpisubEvent::Message{ topic, id, payload } => {
              info!(
                "received message {} on topic {} with payload of {} bytes",
                id, topic, payload.len());
              }
              EpisubEvent::PeerAdded(p) => {
                send_update(
                  &opts.audit,
                  NodeUpdate {
                    node_id: local_peer_id,
                    peer_id: p,
                    event: NodeEvent::Connected,
                  },
                )
                .await?;
              },
              EpisubEvent::PeerRemoved(p) => {
                send_update(
                  &opts.audit,
                  NodeUpdate {
                    node_id: local_peer_id,
                    peer_id: p,
                    event: NodeEvent::Disconnected,
                  },
                )
                .await?;
              },
              _ => {}
            }
          }
          _ => trace!("swarm event: {:?}", event),
        }
      },
      Some(sendmsg) = msg_rx.recv() => {
        if opts.sender {
          swarm.behaviour_mut().publish(&opts.topic[i], sendmsg)?;
          i = (i + 1) % opts.topic.len();
        }
      },
    };
  }
}
