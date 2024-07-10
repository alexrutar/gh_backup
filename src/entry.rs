use std::fmt;

use chrono::{DateTime, FixedOffset};
use serde::{
    de::{SeqAccess, Visitor},
    Deserialize,
};

use crate::LastUpdated;

/// A single repository entry returned by the GitHub API.
#[derive(Debug, Deserialize)]
pub struct Entry {
    #[serde(rename = "nameWithOwner")]
    pub repo: String,
    #[serde(rename = "updatedAt")]
    pub last_updated: DateTime<FixedOffset>,
}

/// A deserializer for the list of repositories returned by the GitHub API.
pub struct DeserializeUserRepos<'a> {
    last_updated: &'a LastUpdated,
    entries: &'a mut Vec<Entry>,
}

impl<'a> DeserializeUserRepos<'a> {
    /// Initialize the deserializer to deserialize all entries which are updated after a certain
    /// date, and append to `entries`.
    pub fn new(last_updated: &'a LastUpdated, entries: &'a mut Vec<Entry>) -> Self {
        Self {
            last_updated,
            entries,
        }
    }
}

impl<'a, 'de> Visitor<'de> for DeserializeUserRepos<'a> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("json returned by `gh repo ls ... --json nameWithOwner --json updatedAt`")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(entry) = seq.next_element::<Entry>()? {
            if self.last_updated.is_outdated(&entry) {
                self.entries.push(entry)
            }
        }

        Ok(())
    }
}
