use libp2p_core::PeerId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EpisubHandlerError {
  #[error("Exceeded maximum transmission size")]
  MaxTransmissionSize,

  /// IO error.
  #[error("IO Error: {0}")]
  Io(#[from] std::io::Error),
}

/// Error associated with publishing a gossipsub message.
#[derive(Debug, Error)]
pub enum PublishError {
  #[error("Attempt to send a message on an unsubscribed topic")]
  TopicNotSubscribed,

  #[error("Exceeded maximum transmission size.")]
  MaxTransmissionSize,
}

/// Errors associated with RPC calls between active nodes
#[derive(Debug, Error)]
pub enum RpcError {
  #[error("Peer Id is malformed")]
  InvalidPeerId,

  #[error("Peer {0} is impersonating {1}")]
  ImpersonatedPeer(PeerId, PeerId),

  #[error("Expected a 16-byte u128")]
  InvalidMessageId,
}

/// Errors associated with converting values from
/// wire format to internal represenation
#[derive(Debug, Error)]
pub enum FormatError {
  #[error("Invalid multihash")]
  Multihash,

  #[error("Invalid multiaddress: {0}")]
  Multiaddr(#[from] multiaddr::Error),
}
