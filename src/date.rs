use std::collections::HashMap;

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use color_eyre::eyre::Result;

use crate::entry::Entry;

/// A record of the previous updates.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct LastUpdated(HashMap<String, DateTime<FixedOffset>>);

impl LastUpdated {
    /// Read the update record from a file.
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        match File::open(path) {
            Ok(file) => Ok(serde_json::from_reader(BufReader::new(file))?),
            Err(_) => Ok(Self::default()),
        }
    }

    /// Write the update record to a file.
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);

        Ok(serde_json::to_writer(writer, &self)?)
    }

    /// Check whether or not an entry is outdated.
    pub fn is_outdated(&self, entry: &Entry) -> bool {
        match self.0.get(&entry.repo) {
            Some(dt) => &entry.last_updated >= dt,
            None => true,
        }
    }

    pub fn update(&mut self, repo: String, at: DateTime<FixedOffset>) {
        self.0.insert(repo, at);
    }
}
