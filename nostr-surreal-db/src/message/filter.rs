use std::collections::HashMap;

use serde::Serialize;

use crate::types::{Tags, Timestamp};

use super::{sender::Sender, wire::{parse_filter_tags, FilterOnWire}};

#[derive(PartialEq, Eq, Debug, Clone, Default, Serialize)]
pub struct Filter {
    pub ids: Vec<[u8; 32]>,
    pub authors: Vec<Sender>,

    pub kinds: Vec<u16>,

    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<u64>,

    pub tags: HashMap<String, Vec<String> >,
}

impl TryFrom<FilterOnWire> for Filter {
    type Error = anyhow::Error;

    fn try_from(value: FilterOnWire) -> Result<Self, Self::Error> {
        let FilterOnWire {
            ids,
            mut authors,
            kinds,
            since,
            until,
            limit,
            tags,
        } = value;

        let mut ids = ids.into_iter().map(|id| id.into()).collect::<Vec<_>>();
        ids.sort(); ids.dedup();

        authors.sort(); authors.dedup();

        let mut kinds = kinds.into_iter().map(|kind| kind.into()).collect::<Vec<_>>();
        kinds.sort(); kinds.dedup();

        Ok(Self {
            ids, authors, kinds,
            since, until, limit,
            tags: parse_filter_tags(tags)?,
        })
    }
}

impl Filter {
    pub fn match_time(&self, timestamp: Timestamp) -> bool {
        let since = self.since.unwrap_or(0);
        let until = self.until.unwrap_or(u64::MAX);

        timestamp >= since && timestamp <= until
    }

    pub fn match_id(&self, id: &[u8; 32]) -> bool {
        self.ids.is_empty() || self.ids.contains(id)
    }

    pub fn match_author(&self, sender: &Sender) -> bool {
        self.authors.is_empty()
            || self.authors.contains(sender)
    }

    pub fn match_kind(&self, kind: u16) -> bool {
        self.kinds.is_empty() || self.kinds.contains(&kind)
    }

    pub fn match_tag(&self, event_tags: &Tags) -> bool {
        // empty tags
        if self.tags.is_empty() {
            return true;
        }

        // event has not tag
        if event_tags.len() == 0 {
            return false;
        }

        let match_one_tag = |event_tags: &Tags, name: &str, list: &[String]| {
            for tag in event_tags {
                if tag.0 == name && list.contains(&tag.1) {
                    return true;
                }
            }

            false
        };

        // all tag must match
        for tag in self.tags.iter() {
            if !match_one_tag(&event_tags, tag.0, &tag.1) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::address;
    use anyhow::Result;

    use super::*;
    use crate::message::{events::Event, wire::EventOnWire};

    #[test]
    fn deser_filter() -> Result<()> {
        // empty
        let note = "{}";
        let filter: FilterOnWire = serde_json::from_str(note)?;
        let filter: Filter = filter.try_into()?;
        assert!(filter.tags.is_empty());
        assert!(filter.ids.is_empty());

        // valid
        let note = r###"
        {
            "ids": ["abababababababababababababababababababababababababababababababab", "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd", "1212121212121212121212121212121212121212121212121212121212121212"],
            "authors": ["010000000000000000000000000000000000000000000000"],
            "kinds": [2, 1],
            "until": 5,
            "since": 3,
            "limit": 6,
            "#d": ["ab", "cd", "12"],
            "#f": ["ab", "cd", "12", "ab"],
            "#b": [],
            "search": "abc",
            "invalid": ["ab", "cd", "12"],
            "_invalid": 123
          }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note)?;
        let filter: Filter = filter.try_into()?;

        let li = vec![[0x12u8; 32], [0xabu8; 32], [0xcdu8; 32]];
        let senders = vec![
            Sender::EoaAddress(address!("0000000000000000000000000000000000000000")),
        ];
        let mut tags = ["ab".to_string(), "cd".to_string(), "12".to_string()].to_vec();
        tags.sort();
        assert_eq!(&filter.ids, &li);
        assert_eq!(&filter.authors, &senders);
        assert_eq!(filter.kinds, vec![1u16, 2u16]);
        assert_eq!(filter.until, Some(5));
        assert_eq!(filter.since, Some(3));
        assert_eq!(filter.limit, Some(6));

        // tag
        assert_eq!(filter.tags.get("d"), Some(&tags));
        // dup
        assert_eq!(filter.tags.get("f"), Some(&tags));
        assert!(filter.tags.get("invalid").is_none());
        assert!(filter.tags.get("_invalid").is_none());
        assert!(filter.tags.get("b").is_none());

        // invalid
        let note = r###"
        {
            "#g": ["ab", "cd", 12]
          }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note)?;
        let filter: Result<Filter> = filter.try_into();
        assert!(filter.is_err());

        let note = r###"
        {
            "#e": ["ab"],
            "#p": ["ab"]
          }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note)?;
        let filter: Result<Filter> = filter.try_into();
        assert!(filter.is_err());

        let note = r###"
        {
            "#e": ["0000000000000000000000000000000000000000000000000000000000000000"],
            "#p": ["0000000000000000000000000000000000000000000000000000000000000000"]
          }
        "###;
        let filter: FilterOnWire = serde_json::from_str(note)?;
        let filter: Filter = filter.try_into()?;
        assert!(filter.tags.get("e").unwrap().contains(&"0000000000000000000000000000000000000000000000000000000000000000".to_string()));
        assert!(filter.tags.get("p").unwrap().contains(&"0000000000000000000000000000000000000000000000000000000000000000".to_string()));
        Ok(())
    }

    fn check_match(
        s: &str,
        matched: bool,
        event: &Event,
    ) -> Result<()> {
        let filter: FilterOnWire = serde_json::from_str(s)?;
        let filter: Filter = filter.try_into()?;

        println!("{:?}", filter);
        assert_eq!(event.match_filter(&filter), matched);
        Ok(())
    }

    #[test]
    fn match_event() -> Result<()> {
        let note = r#"
        {
            "content": "Good morning everyone 😃",
            "created_at": 1680690006,
            "id": "eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d",
            "kind": 1,
            "sender": "010000000000000000000000000000000000000000000000",
            "sig": "ef4ff4f69ac387239eb1401fb07d7a44a5d5d57127e0dc3466a0403cf7d5486b668608ebfcbe9ff1f8d3b5d710545999fe08ee767284ec0b474e4cf92537678f",
            "tags": [["t", "nostr"], ["t", ""], ["expiration", "1"], ["delegation", "8e0d3d3eb2881ec137a11debe736a9086715a8c8beeeda615780064d68bc25dd"]]
          }
        "#;
        let raw_event: EventOnWire = serde_json::from_str(note).unwrap();
        let event: Event = raw_event.try_into().unwrap();

        check_match(
            r###"
        {
        }
        "###,
            true,
            &event,
        )?;

        check_match(
            r###"
        {
            "ids": ["eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d", "0000000000000000000000000000000000000000000000000000000000000000"],
            "authors": ["010000000000000000000000000000000000000000000000", "000000000000000000000000000000000000000000000000000000000000000000000000"],
            "kind": [1, 2],
            "#t": ["nostr", "other"],
            "#subject": ["db", "other"],
            "since": 1680690000,
            "util": 2680690000
        }
        "###,
            true,
            &event,
        )?;

        check_match(
            r###"
        {
            "#t": ["other"]
        }
        "###,
            false,
            &event,
        )?;

        check_match(
            r###"
        {
            "#t": ["nostr"],
            "#r": ["nostr"]
        }
        "###,
            false,
            &event,
        )?;

        check_match(
            r###"
        {
            "ids": ["eb91d677875863c5719e54c8f2841930616464d06f9c791e808bf000d398906d"]
        }
        "###,
            true,
            &event,
        )?;

        check_match(
            r###"
        {
            "ids": ["abababababababababababababababababababababababababababababababab"]
        }
        "###,
            false,
            &event,
        )?;

        Ok(())
    }
}
