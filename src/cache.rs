use libp2p_core::PeerId;
use std::{
  cmp::Ordering,
  collections::{hash_map::Entry, HashMap, VecDeque},
  hash::Hash,
  ops::{Deref, Range},
  sync::Arc,
  time::Instant,
};

use crate::rpc;

pub trait Keyed {
  type Key: Eq + Hash;
  fn key(&self) -> Self::Key;
}

pub struct Timed<T>(T, Instant);

impl<T> Timed<T> {
  pub fn new(value: T) -> Self {
    Self(value, Instant::now())
  }

  pub fn time(&self) -> &Instant {
    &self.1
  }
}

impl<T> Deref for Timed<T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<T> PartialEq for Timed<T> {
  fn eq(&self, other: &Self) -> bool {
    self.time().eq(other.time())
  }
}

impl<T> Eq for Timed<T> {}

impl<T> PartialOrd for Timed<T> {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    self.time().partial_cmp(other.time())
  }
}

impl<T> Ord for Timed<T> {
  fn cmp(&self, other: &Self) -> Ordering {
    self.time().cmp(other.time())
  }
}

pub struct ExpiringCache<T: Keyed + Ord> {
  data: HashMap<T::Key, Arc<Timed<T>>>,
  by_time: VecDeque<Arc<Timed<T>>>,
}

impl<T> ExpiringCache<T>
where
  T: Keyed + Ord,
{
  pub fn new() -> Self {
    Self {
      data: HashMap::new(),
      by_time: VecDeque::new(),
    }
  }

  pub fn insert(&mut self, item: T) -> bool {
    let ptr1 = Arc::new(Timed::new(item));
    let ptr2 = Arc::clone(&ptr1);
    match self.data.entry(ptr1.key()) {
      Entry::Occupied(mut o) => {
        // if the ihave has less hops for the msg than
        // the one we already have, replace it, as it yields
        // a shorter path and lower latency.
        if ptr1.deref().deref() < o.get().deref().deref() {
          o.insert(ptr1);
        }
        true
      }
      Entry::Vacant(v) => {
        v.insert(ptr1);
        self.by_time.push_back(ptr2);
        false
      }
    }
  }

  pub fn get(&self, key: &T::Key) -> Option<&T> {
    self.data.get(key).map(|arc| &**arc.as_ref())
  }

  pub fn iter_range(&self, range: Range<Instant>) -> impl Iterator<Item = &T> {
    let start = self
      .by_time
      .binary_search_by(|x| x.time().cmp(&range.start))
      .unwrap_or_else(|pos| pos);
    let end = self
      .by_time
      .binary_search_by(|x| x.time().cmp(&range.end))
      .unwrap_or_else(|pos| pos);
    self.by_time.range(start..end).map(|item| &***item)
  }

  pub fn remove_older_than(&mut self, since: Instant) {
    while let Some(entry) = self.by_time.front() {
      if entry.time() > &since {
        break;
      }
      self.data.remove(&entry.key());
      self.by_time.pop_front();
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRecord {
  pub id: u128,
  pub hop: u32,
  pub sender: PeerId,
  pub payload: Vec<u8>,
}

impl PartialOrd for MessageRecord {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    self.hop.partial_cmp(&other.hop)
  }
}
impl Ord for MessageRecord {
  fn cmp(&self, other: &Self) -> Ordering {
    self.hop.cmp(&other.hop)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageInfo {
  pub id: u128,
  pub hop: u32,
  pub sender: PeerId,
}

impl PartialOrd for MessageInfo {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    self.hop.partial_cmp(&other.hop)
  }
}
impl Ord for MessageInfo {
  fn cmp(&self, other: &Self) -> Ordering {
    self.hop.cmp(&other.hop)
  }
}

impl Hash for MessageInfo {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.id.hash(state);
  }
}

impl Keyed for MessageRecord {
  type Key = u128;
  fn key(&self) -> Self::Key {
    self.id
  }
}

impl Keyed for MessageInfo {
  type Key = u128;
  fn key(&self) -> Self::Key {
    self.id
  }
}

impl From<MessageRecord> for rpc::Message {
  fn from(record: MessageRecord) -> Self {
    rpc::Message {
      id: record.id.to_le_bytes().to_vec(),
      hop: record.hop,
      payload: record.payload,
    }
  }
}

impl From<&MessageRecord> for rpc::Message {
  fn from(record: &MessageRecord) -> Self {
    record.clone().into()
  }
}

impl From<MessageRecord> for rpc::i_have::MessageRecord {
  fn from(record: MessageRecord) -> Self {
    rpc::i_have::MessageRecord {
      id: record.id.to_le_bytes().to_vec(),
      hop: record.hop,
    }
  }
}

impl From<&MessageRecord> for rpc::i_have::MessageRecord {
  fn from(record: &MessageRecord) -> Self {
    record.clone().into()
  }
}
