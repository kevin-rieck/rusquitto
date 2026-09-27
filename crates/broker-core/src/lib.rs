use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub struct TopicName(String);

#[derive(Debug, PartialEq)]
pub struct TopicNameError;

#[derive(Debug, PartialEq)]
pub struct TopicFilter(String);

#[derive(Debug, PartialEq)]
pub struct TopicFilterError;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SessionId(u64);

#[derive(Debug, PartialEq, Default)]
pub struct TopicIndex {
    ids: HashMap<String, Vec<SessionId>>,
}

impl TryFrom<&str> for TopicName {
    type Error = TopicNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() || value.contains('+') || value.contains('#') || value.contains('\0') {
            return Err(TopicNameError);
        }
        if value.len() > 65_535 {
            return Err(TopicNameError);
        }
        let topic_name = TopicName(value.to_owned());
        Ok(topic_name)
    }
}

impl TryFrom<&str> for TopicFilter {
    type Error = TopicFilterError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(TopicFilterError);
        }
        if value.contains('\0') {
            return Err(TopicFilterError);
        }
        if value.len() > 65_535 {
            return Err(TopicFilterError);
        }
        let mut levels = value.split('/').peekable();

        while let Some(level) = levels.next() {
            if level.contains('+') && level != "+" {
                return Err(TopicFilterError);
            }

            if level.contains('#') && (level != "#" || levels.peek().is_some()) {
                return Err(TopicFilterError);
            }
        }
        Ok(Self(value.to_owned()))
    }
}

impl TopicFilter {
    pub fn matches(&self, name: &TopicName) -> bool {
        if name.0.starts_with('$') && (self.0.starts_with('+') || self.0.starts_with('#')) {
            return false;
        }
        let mut topic_levels = name.0.split('/');
        let mut filter_levels = self.0.split('/');

        loop {
            let next_filter_level = filter_levels.next();
            let next_topic_level = topic_levels.next();

            match (next_filter_level, next_topic_level) {
                (Some(filter), Some(topic)) => {
                    if filter == "#" {
                        return true;
                    }
                    if filter == topic || filter == "+" {
                        continue;
                    }
                    return false;
                }
                (Some(filter), None) => {
                    if filter == "#" {
                        return true;
                    }
                    return false;
                }
                (None, Some(_)) => return false,
                (None, None) => return true,
            }
        }
    }
}

impl TopicIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, session_id: SessionId, filter: TopicFilter) {
        self.ids.entry(filter.0).or_default().push(session_id);
    }

    pub fn matching(&self, topic: &TopicName) -> Vec<SessionId> {
        match self.ids.get(&topic.0) {
            Some(topics) => topics.clone(),
            None => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_topic_name_is_accepted() {
        assert!(TopicName::try_from("sensors/temp").is_ok());
    }

    #[test]
    fn empty_topic_name_is_rejected() {
        assert_eq!(TopicName::try_from(""), Err(TopicNameError));
    }

    #[test]
    fn wildcards_in_topic_names_are_rejected() {
        for value in ["sensors/+", "sensors/#"] {
            assert_eq!(TopicName::try_from(value), Err(TopicNameError));
        }
    }

    #[test]
    fn null_character_in_topic_name_is_rejected() {
        assert_eq!(TopicName::try_from("sensors/\0"), Err(TopicNameError));
    }

    #[test]
    fn exceeding_topic_names_are_rejected() {
        let text = "😀".repeat(16_384);
        assert_eq!(TopicName::try_from(text.as_str()), Err(TopicNameError));
    }

    #[test]
    fn topic_name_at_maximum_byte_length_is_accepted() {
        let text = "a".repeat(65_535);
        assert!(TopicName::try_from(text.as_str()).is_ok());
    }

    #[test]
    fn valid_topic_filter_is_accepted() {
        assert!(TopicFilter::try_from("sensors/+").is_ok());
    }

    #[test]
    fn empty_filter_is_rejected() {
        assert_eq!(TopicFilter::try_from(""), Err(TopicFilterError));
    }

    #[test]
    fn wildcard_must_occupy_whole_level() {
        assert_eq!(
            TopicFilter::try_from("sensors/temp+"),
            Err(TopicFilterError)
        );
    }

    #[test]
    fn hash_wildcard_must_be_final_level() {
        assert_eq!(
            TopicFilter::try_from("sensors/#/temp"),
            Err(TopicFilterError)
        );
    }

    #[test]
    fn hash_wildcard_as_final_level_accepted() {
        assert!(TopicFilter::try_from("sensors/#").is_ok());
    }

    #[test]
    fn topic_filter_rejects_null_character() {
        assert_eq!(TopicFilter::try_from("sensors/\0"), Err(TopicFilterError));
    }

    #[test]
    fn exceeding_topic_filters_are_rejected() {
        let text = "🙄".repeat(16_384);
        assert!(matches!(
            TopicFilter::try_from(text.as_str()),
            Err(TopicFilterError)
        ));
    }

    #[test]
    fn topic_filter_at_maximum_byte_length_is_accepted() {
        let text = "a".repeat(65_535);
        assert!(TopicFilter::try_from(text.as_str()).is_ok());
    }

    #[test]
    fn topic_name_matches_filter() {
        let text = "sensors/temp";
        let topic = TopicName::try_from(text).unwrap();
        let filter = TopicFilter::try_from(text).unwrap();
        assert!(filter.matches(&topic));
    }

    #[test]
    fn topic_matches_plus_wildcard_filter() {
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let filter = TopicFilter::try_from("sensors/+").unwrap();
        assert!(filter.matches(&topic));
    }

    #[test]
    fn topic_does_not_plus_wildcard_if_too_many_levels() {
        let topic = TopicName::try_from("sensors/temp/outside").unwrap();
        let filter = TopicFilter::try_from("sensors/+").unwrap();
        assert!(!filter.matches(&topic));
    }

    #[test]
    fn topic_matches_hash_wildcard() {
        let topic = TopicName::try_from("sensors/outside").unwrap();
        let filter = TopicFilter::try_from("sensors/#").unwrap();
        assert!(filter.matches(&topic));
    }

    #[test]
    fn hash_wildcard_matches_zero_or_more_levels() {
        let topic = TopicName::try_from("sensors").unwrap();
        let filter = TopicFilter::try_from("sensors/#").unwrap();
        assert!(filter.matches(&topic));
    }

    #[test]
    fn plus_wildcard_does_not_match_zero_levels() {
        let topic = TopicName::try_from("sensors").unwrap();
        let filter = TopicFilter::try_from("sensors/+").unwrap();
        assert!(!filter.matches(&topic));
    }

    #[test]
    fn plus_wildcard_matches_empty_topic_level() {
        let topic = TopicName::try_from("sensors/").unwrap();
        let filter = TopicFilter::try_from("sensors/+").unwrap();
        assert!(filter.matches(&topic));
    }

    #[test]
    fn system_topics_not_matched_by_toplevel_wildcard() {
        let topic = TopicName::try_from("$SYS/uptime").unwrap();
        let filter = TopicFilter::try_from("#").unwrap();
        assert!(!filter.matches(&topic));
    }

    #[test]
    fn system_topics_matched_by_wildcard_below_toplevel() {
        let topic = TopicName::try_from("$SYS/uptime").unwrap();
        let filter = TopicFilter::try_from("$SYS/#").unwrap();
        assert!(filter.matches(&topic));
    }

    #[test]
    fn exact_matches_work() {
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let filter = TopicFilter::try_from("sensors/humidity").unwrap();
        assert!(!filter.matches(&topic));
    }

    #[test]
    fn topic_index_returns_correct_session_id() {
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let filter = TopicFilter::try_from("sensors/temp").unwrap();
        let session_id = SessionId(1);
        let mut index = TopicIndex::new();
        index.insert(session_id, filter);
        assert_eq!(index.matching(&topic), vec![session_id]);
    }
}
