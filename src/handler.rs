use crate::{
  codec::EpisubCodec, error::EpisubHandlerError, protocol::EpisubProtocol, rpc,
};
use asynchronous_codec::Framed;
use futures::{Sink, StreamExt};
use libp2p_core::{InboundUpgrade, OutboundUpgrade};
use libp2p_swarm::handler::{FullyNegotiatedInbound, FullyNegotiatedOutbound, DialUpgradeError, ListenUpgradeError, InboundUpgradeSend, ProtocolSupport};
use libp2p_swarm::{ handler::ConnectionEvent,
   Stream, ConnectionHandler, 
  SubstreamProtocol, ConnectionHandlerEvent, StreamUpgradeError
};
use libp2p_swarm::{StreamProtocol,AddressChange};
use std::{
  collections::VecDeque,
  io,
  pin::Pin,
  task::{Context, Poll},
};
use tracing::{error, warn};

/// State of the inbound substream, opened either by us or by the remote.
enum InboundSubstreamState {
  /// Waiting for a message from the remote. The idle state for an inbound substream.
  WaitingInput(Framed<Stream, EpisubCodec>),
  /// The substream is being closed.
  Closing(Framed<Stream, EpisubCodec>),
  /// An error occurred during processing.
  Poisoned,
}

/// State of the outbound substream, opened either by us or by the remote.
enum OutboundSubstreamState {
  // upgrade requested and waiting for the upgrade to be negotiated.
  SubstreamRequested,
  /// Waiting for the user to send a message. The idle state for an outbound substream.
  WaitingOutput(Framed<Stream, EpisubCodec>),
  /// Waiting to send a message to the remote.
  PendingSend(Framed<Stream, EpisubCodec>, crate::rpc::Rpc),
  /// Waiting to flush the substream so that the data arrives to the remote.
  PendingFlush(Framed<Stream, EpisubCodec>),
  /// The substream is being closed. Used by either substream.
  _Closing(Framed<Stream, EpisubCodec>),
  /// An error occurred during processing.
  Poisoned,
}

/// Protocol handler that manages a single long-lived substream with a peer
pub struct EpisubHandler {
  /// Upgrade configuration for the episub protocol.
  listen_protocol: SubstreamProtocol<EpisubProtocol, ()>,
  /// The single long-lived outbound substream.
  outbound_substream: Option<OutboundSubstreamState>,
  /// The single long-lived inbound substream.
  inbound_substream: Option<InboundSubstreamState>,
  /// Whether we want the peer to have strong live connection to us.
  /// This changes when a peer is moved from the active view to the passive view.
  keep_alive: bool,
  /// The list of messages scheduled to be sent to this peer
  outbound_queue: VecDeque<rpc::Rpc>,
}

type EpisubHandlerEvent = ConnectionHandlerEvent<
  <EpisubHandler as ConnectionHandler>::OutboundProtocol,
  <EpisubHandler as ConnectionHandler>::OutboundOpenInfo,
  <EpisubHandler as ConnectionHandler>::ToBehaviour  
>;

impl EpisubHandler {
  // temporary: used only for shuffle reply, then the connection is closed
  pub fn new(max_transmit_size: usize, temporary: bool) -> Self {
    Self {
      listen_protocol: SubstreamProtocol::new(
        EpisubProtocol::new(max_transmit_size),
        (),
      ),
      keep_alive: match temporary {
        false => true,
        true => false,
      },
      outbound_substream: None,
      inbound_substream: None,
      outbound_queue: VecDeque::new(),
    }
  }
}

impl ConnectionHandler for EpisubHandler {

  type FromBehaviour = rpc::Rpc;
  type ToBehaviour = rpc::Rpc;
  type InboundOpenInfo = ();
  type InboundProtocol = EpisubProtocol;
  type OutboundOpenInfo = ();
  type OutboundProtocol = EpisubProtocol;

  fn listen_protocol(
    &self,
  ) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
    self.listen_protocol.clone()
  }

    /// Informs the handler about an event from the [`NetworkBehaviour`](super::NetworkBehaviour).
    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour)
    {
      if !self.keep_alive {
        // temporary connection are only for
        // shuffle replies. Don't permit any
        // outgoing message other than shuffle
        // reply
       
        if let rpc::Rpc {
          action: Some(rpc::rpc::Action::ShuffleReply(_)),
          ..
        } = _event
        {
          self.outbound_queue.push_back(_event);
        }
        
      } else {
        self.outbound_queue.push_back(_event);
      }
    }

  fn on_connection_event(
    &mut self,
    event: ConnectionEvent<
        Self::InboundProtocol,
        Self::OutboundProtocol,
        Self::InboundOpenInfo,
        Self::OutboundOpenInfo,
    >,
){

  match  event{
  
    ConnectionEvent::FullyNegotiatedInbound( FullyNegotiatedInbound{protocol,info:_})=>
    {
      self.inbound_substream =
      Some(InboundSubstreamState::WaitingInput(protocol))
    },
    ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound{protocol,info:_})=>
    {
      self.outbound_substream =
      Some(OutboundSubstreamState::WaitingOutput(protocol));
    },
    ConnectionEvent:: DialUpgradeError(DialUpgradeError{info:_, error}) =>
    {
      warn!("dial upgrade error: {:?}", error);
    },
    ConnectionEvent::ListenUpgradeError(ListenUpgradeError{info:_,error})=>
    {
      warn!("listen upgrade error: {:?}", error);
    },
    ConnectionEvent::AddressChange(_a)=>
    {

    },
    ConnectionEvent::LocalProtocolsChange(_p)=>{
    },
    ConnectionEvent::RemoteProtocolsChange(_p)=>
    {},
    _ =>{}
  }
}

fn connection_keep_alive(&self) -> bool {
  self.keep_alive
}

  fn poll(&mut self, cx: &mut Context<'_>) -> Poll<EpisubHandlerEvent> {
    // process inbound stream first
    let inbound_poll = self.process_inbound_poll(cx);
    if !matches!(inbound_poll, Poll::<EpisubHandlerEvent>::Pending) {
      return inbound_poll;
    }
    // then process outbound steram
    let outbound_poll = self.process_outbound_poll(cx);
    if !matches!(outbound_poll, Poll::<EpisubHandlerEvent>::Pending) {
      return outbound_poll;
    }
    // nothing to communicate to the runtime for this connection.
    Poll::Pending
  }
}

impl EpisubHandler {
  fn process_inbound_poll(
    &mut self,
    cx: &mut Context<'_>,
  ) -> Poll<EpisubHandlerEvent> {
    loop {
      match std::mem::replace(
        &mut self.inbound_substream,
        Some(InboundSubstreamState::Poisoned),
      ) {
        Some(InboundSubstreamState::WaitingInput(mut substream)) => {
          match substream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(message))) => {
              self.inbound_substream =
                Some(InboundSubstreamState::WaitingInput(substream));
              return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(message));
            }
            Poll::Ready(Some(Err(error))) => {
              warn!("inbound stream error: {:?}", error);
            }
            Poll::Ready(None) => {
              warn!("Peer closed their outbound stream");
              self.inbound_substream =
                Some(InboundSubstreamState::Closing(substream));
            }
            Poll::Pending => {
              self.inbound_substream =
                Some(InboundSubstreamState::WaitingInput(substream));
              break;
            }
          }
        }
        Some(InboundSubstreamState::Closing(mut substream)) => {
          match Sink::poll_close(Pin::new(&mut substream), cx) {
            Poll::Ready(res) => {
              if let Err(e) = res {
                // Don't close the connection but just drop the inbound substream.
                // In case the remote has more to send, they will open up a new
                // substream.
                warn!("Inbound substream error while closing: {:?}", e);
              }
              self.inbound_substream = None;
              if self.outbound_substream.is_none() {
                self.keep_alive = false;
              }
              break;
            }
            Poll::Pending => {
              self.inbound_substream =
                Some(InboundSubstreamState::Closing(substream));
              break;
            }
          }
        }
        Some(InboundSubstreamState::Poisoned) => {
          unreachable!("Error occurred during inbound stream processing");
        }
        None => {
          self.inbound_substream = None;
          break;
        }
      }
    }
    Poll::Pending
  }

  fn process_outbound_poll(
    &mut self,
    cx: &mut Context<'_>,
  ) -> Poll<EpisubHandlerEvent> {
    loop {
      match std::mem::replace(
        &mut self.outbound_substream,
        Some(OutboundSubstreamState::Poisoned),
      ) {
        Some(OutboundSubstreamState::WaitingOutput(substream)) => {
          if let Some(msg) = self.outbound_queue.pop_front() {
            self.outbound_queue.shrink_to_fit();
            self.outbound_substream =
              Some(OutboundSubstreamState::PendingSend(substream, msg));
          } else {
            self.outbound_substream =
              Some(OutboundSubstreamState::WaitingOutput(substream));
            break;
          }
        }
        Some(OutboundSubstreamState::PendingSend(mut substream, message)) => {
          match Sink::poll_ready(Pin::new(&mut substream), cx) {
            Poll::Ready(Ok(())) => {
              match Sink::start_send(Pin::new(&mut substream), message) {
                Ok(()) => {
                  self.outbound_substream =
                    Some(OutboundSubstreamState::PendingFlush(substream));
                }
                Err(EpisubHandlerError::MaxTransmissionSize) => {
                  error!("Message exceeds the maximum transmission size and was dropped.");
                  self.outbound_substream =
                    Some(OutboundSubstreamState::WaitingOutput(substream));
                }
                Err(e) => {
                  error!("Error sending message: {}", e);
                  self.keep_alive=false;                  
                  return Poll::Ready(ConnectionHandlerEvent::ReportRemoteProtocols(ProtocolSupport::Removed( vec![StreamProtocol::new("/beta/episub/1.0.0")].into_iter().collect())))
                }
              }
            }
            Poll::Ready(Err(e)) => {
              error!("outbound substream error while sending message: {:?}", e);
              self.keep_alive=false;
              return Poll::Ready(ConnectionHandlerEvent::ReportRemoteProtocols(ProtocolSupport::Removed( vec![StreamProtocol::new("/beta/episub/1.0.0")].into_iter().collect())))
            }
            Poll::Pending => {
              self.keep_alive = true;
              self.outbound_substream =
                Some(OutboundSubstreamState::PendingSend(substream, message));
              break;
            }
          }
        }
        Some(OutboundSubstreamState::PendingFlush(mut substream)) => {
          match Sink::poll_flush(Pin::new(&mut substream), cx) {
            Poll::Ready(Ok(())) => {
              self.outbound_substream =
                Some(OutboundSubstreamState::WaitingOutput(substream));
            }
            Poll::Ready(Err(_e)) => {
              self.keep_alive=false;
              return Poll::Ready(ConnectionHandlerEvent::ReportRemoteProtocols(ProtocolSupport::Removed( vec![StreamProtocol::new("/beta/episub/1.0.0")].into_iter().collect())))        
            }
            Poll::Pending => {
              self.keep_alive = true;
              self.outbound_substream =
                Some(OutboundSubstreamState::PendingFlush(substream));
              break;
            }
          }
        }
        Some(OutboundSubstreamState::_Closing(mut substream)) => {
          match Sink::poll_close(Pin::new(&mut substream), cx) {
            Poll::Ready(Ok(())) => {
              self.outbound_substream = None;
              if self.inbound_substream.is_none() {
                self.keep_alive = false;
              }
              break;
            }
            Poll::Ready(Err(e)) => {
              warn!("Outbound substream error while closing: {:?}", e);
              self.keep_alive = false;
              return Poll::Ready(ConnectionHandlerEvent::ReportRemoteProtocols(ProtocolSupport::Removed( vec![StreamProtocol::new("/beta/episub/1.0.0")].into_iter().collect())))
            }
            Poll::Pending => {
              self.keep_alive =false;
              self.outbound_substream =
                Some(OutboundSubstreamState::_Closing(substream));
              break;
            }
          }
        }
        Some(OutboundSubstreamState::SubstreamRequested) => {
          self.outbound_substream =
            Some(OutboundSubstreamState::SubstreamRequested);
          break;
        }
        Some(OutboundSubstreamState::Poisoned) => {
          unreachable!("Error occurred during outbound stream processing");
        }
        None => {
          self.outbound_substream =
            Some(OutboundSubstreamState::SubstreamRequested);
          return Poll::Ready(
            ConnectionHandlerEvent::OutboundSubstreamRequest {
              protocol: self.listen_protocol.clone(),
            },
          );
        }
      }
    }

    Poll::Pending
  }
}
