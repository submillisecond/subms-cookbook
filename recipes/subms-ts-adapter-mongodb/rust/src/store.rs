//! The store seam. `TsMongoAdapter` talks to a `TsMongoStore`, never to a
//! driver directly, so the mapping is exercised end to end without a server.
//! The default `InMemoryMongoStore` is a hermetic test double that also records
//! a change log for the change-data-capture surface.

use std::collections::BTreeMap;
use std::sync::Mutex;

use bson::{Bson, Document};

use crate::error::TsMongoError;

/// A captured change. The in-memory store records one per inserted document;
/// a live driver would surface these off a MongoDB change stream.
#[derive(Clone, Debug, PartialEq)]
pub enum TsChangeEvent {
    Insert { collection: String, doc: Document },
}

/// The minimal document-store surface the adapter needs. A real driver impl
/// lives behind the `driver` feature; tests use [`InMemoryMongoStore`].
pub trait TsMongoStore {
    fn insert_many(&self, collection: &str, docs: Vec<Document>) -> Result<u64, TsMongoError>;
    fn find_all(&self, collection: &str) -> Result<Vec<Document>, TsMongoError>;
    fn find_one(&self, collection: &str, id: &Bson) -> Result<Option<Document>, TsMongoError>;
    fn create_index(&self, collection: &str, keys: Document) -> Result<(), TsMongoError>;
    fn collections(&self) -> Result<Vec<String>, TsMongoError>;

    /// Drain captured change events. Default: none (a driver wires this to a
    /// change stream, the live-network path excluded from coverage).
    fn drain_changes(&self) -> Result<Vec<TsChangeEvent>, TsMongoError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct Inner {
    collections: BTreeMap<String, Vec<Document>>,
    indexes: BTreeMap<String, Vec<Document>>,
    changes: Vec<TsChangeEvent>,
}

/// A hermetic in-memory document store. Holds collections as ordered vectors,
/// records created indexes, and logs every insert as a change event.
#[derive(Default)]
pub struct InMemoryMongoStore {
    inner: Mutex<Inner>,
}

impl InMemoryMongoStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of documents currently held in a collection.
    pub fn count(&self, collection: &str) -> usize {
        self.inner
            .lock()
            .unwrap()
            .collections
            .get(collection)
            .map(Vec::len)
            .unwrap_or(0)
    }

    /// Index key documents recorded for a collection.
    pub fn indexes(&self, collection: &str) -> Vec<Document> {
        self.inner
            .lock()
            .unwrap()
            .indexes
            .get(collection)
            .cloned()
            .unwrap_or_default()
    }
}

impl TsMongoStore for InMemoryMongoStore {
    fn insert_many(&self, collection: &str, docs: Vec<Document>) -> Result<u64, TsMongoError> {
        let mut g = self.inner.lock().unwrap();
        let n = docs.len() as u64;
        let bucket = g.collections.entry(collection.to_string()).or_default();
        for d in &docs {
            bucket.push(d.clone());
        }
        for d in docs {
            g.changes.push(TsChangeEvent::Insert {
                collection: collection.to_string(),
                doc: d,
            });
        }
        Ok(n)
    }

    fn find_all(&self, collection: &str) -> Result<Vec<Document>, TsMongoError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .collections
            .get(collection)
            .cloned()
            .unwrap_or_default())
    }

    fn find_one(&self, collection: &str, id: &Bson) -> Result<Option<Document>, TsMongoError> {
        let g = self.inner.lock().unwrap();
        Ok(g.collections
            .get(collection)
            .and_then(|docs| docs.iter().find(|d| d.get("_id") == Some(id)).cloned()))
    }

    fn create_index(&self, collection: &str, keys: Document) -> Result<(), TsMongoError> {
        self.inner
            .lock()
            .unwrap()
            .indexes
            .entry(collection.to_string())
            .or_default()
            .push(keys);
        Ok(())
    }

    fn collections(&self) -> Result<Vec<String>, TsMongoError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .collections
            .keys()
            .cloned()
            .collect())
    }

    fn drain_changes(&self) -> Result<Vec<TsChangeEvent>, TsMongoError> {
        let mut g = self.inner.lock().unwrap();
        Ok(std::mem::take(&mut g.changes))
    }
}
