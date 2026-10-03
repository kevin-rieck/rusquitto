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
    root: Node,
}

#[derive(Default, Debug, PartialEq)]
struct Node {
    children: HashMap<String, Node>,
    ids: Vec<SessionId>,
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
        let mut node = &mut self.root;
        for level in filter.0.split('/') {
            node = node.children.entry(level.to_owned()).or_default();
        }
        node.ids.push(session_id);
    }

    pub fn matching(&self, topic: &TopicName) -> Vec<SessionId> {
        fn visit(node: &Node, levels: &[&str], at_root: bool, ids: &mut Vec<SessionId>) {
            if (!at_root || !levels.first().is_some_and(|level| level.starts_with('$')))
                && let Some(child) = node.children.get("#")
            {
                ids.extend(child.ids.iter().copied());
            }

            let Some((level, rest)) = levels.split_first() else {
                ids.extend(node.ids.iter().copied());
                return;
            };

            if let Some(child) = node.children.get(*level) {
                visit(child, rest, false, ids);
            }

            if (!at_root || !level.starts_with('$'))
                && let Some(child) = node.children.get("+")
            {
                visit(child, rest, false, ids);
            }
        }

        let levels: Vec<_> = topic.0.split('/').collect();
        let mut ids = Vec::new();
        visit(&self.root, &levels, true, &mut ids);
        ids
    }

    pub fn remove(&mut self, session_id: SessionId, filter: &TopicFilter) {
        fn prune(node: &mut Node, remaining_levels: &[&str], session_id: SessionId) -> bool {
            match remaining_levels.split_first() {
                None => {
                    node.ids.retain(|id| *id != session_id);
                    node.ids.is_empty() && node.children.is_empty()
                }
                Some((level, rest)) => match node.children.get_mut(*level) {
                    Some(child) => {
                        let needs_pruning = prune(child, rest, session_id);
                        if needs_pruning {
                            node.children.remove(*level);
                        }
                        node.ids.is_empty() && node.children.is_empty()
                    }
                    None => false,
                },
            }
        }
        let levels: Vec<_> = filter.0.split('/').collect();
        prune(&mut self.root, &levels, session_id);
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

    #[test]
    fn matching_topic_returns_session_id() {
        let id = SessionId(1);
        let filter = TopicFilter::try_from("sensors/+").unwrap();
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let mut index = TopicIndex::new();
        index.insert(id, filter);
        assert_eq!(index.matching(&topic), vec![id]);
    }

    #[test]
    fn matching_hash_wildcard_returns_id() {
        let id = SessionId(1);
        let filter = TopicFilter::try_from("sensors/#").unwrap();
        let topic = TopicName::try_from("sensors").unwrap();
        let mut index = TopicIndex::new();
        index.insert(id, filter);
        assert_eq!(index.matching(&topic), vec![id]);
    }

    #[test]
    fn session_id_not_returned_after_removal() {
        let topic = "sensors/temp";
        let filter = "sensors/temp";
        let mut index = TopicIndex::new();
        index.insert(SessionId(1), TopicFilter::try_from(filter).unwrap());
        index.insert(SessionId(2), TopicFilter::try_from(filter).unwrap());
        index.remove(SessionId(1), &TopicFilter::try_from(filter).unwrap());
        assert_eq!(
            index.matching(&TopicName::try_from(topic).unwrap()),
            vec![SessionId(2)]
        )
    }

    #[test]
    fn empty_index_paths_reclaimed_after_removal() {
        let filter = "sensors/temp";
        let mut index = TopicIndex::new();
        index.insert(SessionId(1), TopicFilter::try_from(filter).unwrap());
        index.remove(SessionId(1), &TopicFilter::try_from(filter).unwrap());
        assert!(index.root.children.is_empty());
    }
}
