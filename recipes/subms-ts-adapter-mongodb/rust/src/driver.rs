//! The live-network store: the official MongoDB sync driver behind the
//! `TsMongoStore` seam. Compiled only under the `driver` feature and excluded
//! from coverage - it is the one class that requires a running server. The
//! driver owns connection handling and SCRAM authentication; this file is just
//! the thin map from the seam's methods onto its sync API.

use bson::{Bson, Document, doc};
use mongodb::IndexModel;
use mongodb::sync::{Client, Database};

use crate::error::TsMongoError;
use crate::store::TsMongoStore;

/// A `TsMongoStore` backed by a live MongoDB deployment.
pub struct DriverMongoStore {
    db: Database,
}

impl DriverMongoStore {
    /// Connect to `uri` and select database `db`. The driver parses the
    /// connection string, negotiates auth, and pools connections lazily.
    pub fn connect(uri: &str, db: &str) -> Result<Self, TsMongoError> {
        let client = Client::with_uri_str(uri).map_err(|e| TsMongoError::store(e.to_string()))?;
        Ok(Self {
            db: client.database(db),
        })
    }
}

impl TsMongoStore for DriverMongoStore {
    fn insert_many(&self, collection: &str, docs: Vec<Document>) -> Result<u64, TsMongoError> {
        if docs.is_empty() {
            return Ok(0);
        }
        let coll = self.db.collection::<Document>(collection);
        let res = coll
            .insert_many(docs)
            .run()
            .map_err(|e| TsMongoError::store(e.to_string()))?;
        Ok(res.inserted_ids.len() as u64)
    }

    fn find_all(&self, collection: &str) -> Result<Vec<Document>, TsMongoError> {
        let coll = self.db.collection::<Document>(collection);
        let cursor = coll
            .find(doc! {})
            .run()
            .map_err(|e| TsMongoError::store(e.to_string()))?;
        let mut out = Vec::new();
        for r in cursor {
            out.push(r.map_err(|e| TsMongoError::store(e.to_string()))?);
        }
        Ok(out)
    }

    fn find_one(&self, collection: &str, id: &Bson) -> Result<Option<Document>, TsMongoError> {
        let coll = self.db.collection::<Document>(collection);
        coll.find_one(doc! { "_id": id.clone() })
            .run()
            .map_err(|e| TsMongoError::store(e.to_string()))
    }

    fn create_index(&self, collection: &str, keys: Document) -> Result<(), TsMongoError> {
        let coll = self.db.collection::<Document>(collection);
        let model = IndexModel::builder().keys(keys).build();
        coll.create_index(model)
            .run()
            .map_err(|e| TsMongoError::store(e.to_string()))?;
        Ok(())
    }

    fn collections(&self) -> Result<Vec<String>, TsMongoError> {
        self.db
            .list_collection_names()
            .run()
            .map_err(|e| TsMongoError::store(e.to_string()))
    }
}
