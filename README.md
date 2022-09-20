# Libp2p-episub: Proximity Aware Epidemic PubSub for libp2p

This Rust library implements a `libp2p` behaviour for efficient large-scale pub/sub protocol based on ideas the following academic papers:

- HyParView: For topic-peer-membership management and node discovery
- Epidemic Broadcast Trees: For constructing efficient broadcast trees and efficient content dessamination
- GoCast: Gossip-Enhanced Overlay Multicast for Fast and Dependable Group Communication

Originally [speced by @vyzo](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/episub.md) as a successor of Gossipsub.

## Running a topology with 201 peers:

```
$ docker-compose up --scale node_t0=200 --remove-orphans
```


## Testing and visualizing p2p network

First start the audit node and open it in the browser on port 80, you should see a `P2P Protocols Lab` empty page.

```
$ docker-compose up -d node_audit
```

Then start the bootstrap node

```
$ docker-compose up -d node_0
```

You should see it immediately showing up in the labs page, then start 50 peer nodes and watch the network discover itself.

```
$ docker-compose up --scale node_t0=50
```



## Usage Examples


```rust
let local_key = identity::Keypair::generate_ed25519();
let local_peer_id = PeerId::from(local_key.public());
let transport = libp2p::development_transport(local_key.clone()).await?;

// Create a Swarm to manage peers and events
let mut swarm = libp2p::Swarm::new(transport, Episub::new(), local_peer_id);

// Listen on all interfaces and whatever port the OS assigns
swarm
  .listen_on("/ip4/0.0.0.0/tcp/4001".parse().unwrap())
  .unwrap();

// subscribe to the topic specified on the command line
swarm.behaviour_mut().subscribe(opts.topic);
swarm.dial(bootstrap).unwrap()

while let Some(event) = swarm.next().await {
  match event {
     SwarmEvent::Behaviour(EpisubEvent::Message(m, t)) => {
        println!("got a message: {:?} on topic {}", m, t);
     }
     SwarmEvent::Behaviour(EpisubEvent::Subscribed(t)) => {}
     SwarmEvent::Behaviour(EpisubEvent::Unsubscribed(t)) => {}
     SwarmEvent::Behaviour(EpisubEvent::ActivePeerAdded(p)) => {}
     SwarmEvent::Behaviour(EpisubEvent::ActivePeerRemoved(p)) => {}
  }
}
```
