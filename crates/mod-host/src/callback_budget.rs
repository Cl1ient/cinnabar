//! A deterministic fuel allowance shared by every callback in a client frame.

use std::sync::{Arc, Mutex};

/// Several instances share one allowance; callback and load limits still apply per instance.
#[derive(Clone)]
pub struct CallbackBudget(Arc<Mutex<u64>>);
impl CallbackBudget {
    /// Creates a frame allowance measured in Wasmtime fuel units.
    pub fn new(fuel: u64) -> Self {
        Self(Arc::new(Mutex::new(fuel)))
    }
    /// Starts a new frame before any guest input or event callback.
    pub fn reset(&self, fuel: u64) {
        *self.0.lock().expect("mod callback budget") = fuel;
    }
    /// Reserves a callback's maximum allowance, without exceeding the remaining frame fuel.
    pub(crate) fn reserve(&self, fuel: u64) -> u64 {
        let mut remaining = self.0.lock().expect("mod callback budget");
        let reserved = fuel.min(*remaining);
        *remaining -= reserved;
        reserved
    }
    /// Returns fuel which a completed callback did not use.
    pub(crate) fn refund(&self, fuel: u64) {
        *self.0.lock().expect("mod callback budget") += fuel;
    }
    /// Reports the allowance remaining after completed callbacks.
    pub fn remaining(&self) -> u64 {
        *self.0.lock().expect("mod callback budget")
    }
}
