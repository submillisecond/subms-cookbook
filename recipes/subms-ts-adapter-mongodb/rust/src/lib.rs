//! `subms-ts-adapter-mongodb` - a MongoDB adapter for the submillisecond cookbook
//! timeseries arc.
//!
//! The tested contract is the BSON document mapping ([`doc`]) plus an injectable
//! [`TsMongoStore`] seam. The default [`InMemoryMongoStore`] runs the whole
//! write / read / index / change-capture path with no server; the official
//! driver is an opt-in `driver` feature (the live-network leg, excluded from
//! coverage).
//!
//! Mapping: each `TsSeries<f64>` is one `ts_<sid>` collection of point documents
//! `{ _id: { sid, ts }, v }`, with series identity (name, tags, schema) in a
//! sidecar `ts_meta` document keyed by the numeric series id.

mod doc;
mod error;
mod store;

#[cfg(feature = "driver")]
mod driver;

#[cfg(feature = "harness")]
pub mod recipe;

pub use doc::{
    META_COLLECTION, doc_from_bytes, doc_to_bytes, meta_doc, meta_from_doc, point_collection,
    point_doc, point_from_doc, series_from_docs, series_to_docs,
};
pub use error::TsMongoError;
pub use store::{InMemoryMongoStore, TsChangeEvent, TsMongoStore};

#[cfg(feature = "driver")]
pub use driver::DriverMongoStore;

use bson::{Bson, doc};
use subms_ts::{TsCollection, TsSeries};

/// A MongoDB adapter parameterised over a store.
pub struct TsMongoAdapter<S: TsMongoStore> {
    store: S,
}

impl<S: TsMongoStore> TsMongoAdapter<S> {
    /// Build over a caller-supplied store (the injection point for tests and
    /// for the live driver).
    pub fn with_store(store: S) -> Self {
        Self { store }
    }

    /// Borrow the underlying store (the inspection point under test).
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Write one series: its sidecar metadata document plus one document per
    /// point. Returns the number of point documents written.
    pub fn write_series(&self, series: &TsSeries<f64>) -> Result<u64, TsMongoError> {
        let (meta, points) = series_to_docs(series);
        self.store.insert_many(META_COLLECTION, vec![meta])?;
        if points.is_empty() {
            return Ok(0);
        }
        let sid = series.metadata().map(|m| m.id).unwrap_or(0);
        self.store.insert_many(&point_collection(sid), points)
    }

    /// Write every series in a collection. Returns the total point count.
    pub fn write_collection(&self, coll: &TsCollection<f64>) -> Result<u64, TsMongoError> {
        let mut total = 0;
        for series in coll.series() {
            total += self.write_series(series)?;
        }
        Ok(total)
    }

    /// Read one series back by its numeric id.
    pub fn read_series(&self, series_id: u64) -> Result<TsSeries<f64>, TsMongoError> {
        let meta = self
            .store
            .find_one(META_COLLECTION, &Bson::Int64(series_id as i64))?
            .ok_or_else(|| TsMongoError::mapping(format!("no metadata for series {series_id}")))?;
        let points = self.store.find_all(&point_collection(series_id))?;
        series_from_docs(&meta, &points)
    }

    /// Read every stored series into a collection.
    pub fn read_collection(&self) -> Result<TsCollection<f64>, TsMongoError> {
        let metas = self.store.find_all(META_COLLECTION)?;
        let mut out = TsCollection::<f64>::new();
        for meta in &metas {
            let m = meta_from_doc(meta)?;
            let sid = m.id;
            let points = self.store.find_all(&point_collection(sid))?;
            let series = series_from_docs(meta, &points)?;
            out.register(m)
                .map_err(|e| TsMongoError::mapping(format!("register failed: {e:?}")))?;
            for p in series.iter() {
                out.push(sid, p.ts, p.value)
                    .map_err(|e| TsMongoError::mapping(format!("push failed: {e:?}")))?;
            }
        }
        Ok(out)
    }

    /// Ensure the `(_id.sid, _id.ts)` compound index exists on every point
    /// collection currently present.
    pub fn ensure_indexes(&self) -> Result<(), TsMongoError> {
        for name in self.store.collections()? {
            if name.starts_with("ts_") && name != META_COLLECTION {
                self.store
                    .create_index(&name, doc! { "_id.sid": 1, "_id.ts": 1 })?;
            }
        }
        Ok(())
    }

    /// Drain captured change events (the change-data-capture surface).
    pub fn poll_changes(&self) -> Result<Vec<TsChangeEvent>, TsMongoError> {
        self.store.drain_changes()
    }
}
