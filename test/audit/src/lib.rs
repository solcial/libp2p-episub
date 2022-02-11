use libp2p::core::PeerId;
use serde::Serialize;

#[derive(Debug, Copy, Clone, Serialize)]
pub enum NodeEvent {
  None,
  Up,
  Down,
  Connected,
  Disconnected,
  Graft,
  Prune,
  IHave,
  Message,
}

#[derive(Debug, Copy, Clone)]
#[repr(packed)]
pub struct NodeUpdate {
  pub node_id: PeerId,
  pub peer_id: PeerId,
  pub event: NodeEvent,
}

impl Default for NodeUpdate {
  fn default() -> Self {
    Self {
      node_id: PeerId::random(),
      peer_id: PeerId::random(),
      event: NodeEvent::None,
    }
  }
}
