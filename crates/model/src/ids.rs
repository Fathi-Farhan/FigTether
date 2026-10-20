//! Typed identifiers.
//!
//! Identifiers are opaque on the wire. A `SessionId` is a UUID minted per
//! plugin run or tab; it never encodes a file name, a `fileKey`, or a user
//! identity (`D05`, `ADR-010`). A node id from Figma is a plain `String` and
//! is never parsed as a number.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[schemars(transparent)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Mint a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wrap an existing UUID.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// The raw UUID.
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    };
}

id_type!(SessionId, "Identity of one plugin run or tab.");
id_type!(SnapshotId, "Identity of one immutable capture.");
id_type!(ObservationId, "Identity of a single observation window.");
id_type!(GrantId, "Identity of one authorization grant.");
id_type!(AssetId, "Opaque identifier of one exported asset.");
id_type!(JobId, "Identity of one scheduled job.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_strings() {
        let id = SessionId::new();
        let text = id.to_string();
        let parsed: SessionId = text.parse().expect("uuid parses");
        assert_eq!(id, parsed);
    }

    #[test]
    fn ids_serialize_transparently() {
        let id = SnapshotId::new();
        let json = serde_json::to_string(&id).expect("serializes");
        assert_eq!(json, format!("\"{id}\""));
    }

    #[test]
    fn distinct_ids_are_distinct() {
        assert_ne!(SessionId::new(), SessionId::new());
    }
}
