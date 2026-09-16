//! Application-level shared state.

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// A type-indexed map of shared application state. Registered once via
/// [`App::state`](crate::App::state), read per request via
/// [`Context::state`](crate::Context::state) — by reference, zero clones.
pub type StateMap = HashMap<TypeId, Box<dyn Any + Send + Sync>>;

pub(crate) fn insert<T: Any + Send + Sync>(map: &mut StateMap, value: T) {
  map.insert(TypeId::of::<T>(), Box::new(value));
}

pub(crate) fn get<T: Any + Send + Sync>(map: &StateMap) -> Option<&T> {
  map
    .get(&TypeId::of::<T>())
    .and_then(|boxed| boxed.downcast_ref::<T>())
}

/// A per-request type-indexed store (unlike `http::Extensions`, values
/// need not be `Clone`).
#[derive(Default)]
pub(crate) struct TypeMap {
  map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl TypeMap {
  pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
    self.map.insert(TypeId::of::<T>(), Box::new(value));
  }

  pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
    self
      .map
      .get(&TypeId::of::<T>())
      .and_then(|boxed| boxed.downcast_ref::<T>())
  }
}
