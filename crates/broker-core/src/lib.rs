#[derive(Debug, PartialEq)]
pub struct TopicName(String);

#[derive(Debug, PartialEq)]
pub struct TopicNameError;

#[derive(Debug, PartialEq)]
pub struct TopicFilter(String);

#[derive(Debug, PartialEq)]
pub struct TopicFilterError;

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
}
