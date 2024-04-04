use anyhow::anyhow;
use nostr_sdk::Report;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModerationCategory {
    Hate,
    HateThreatening,
    Harassment,
    HarassmentThreatening,
    SelfHarm,
    SelfHarmIntent,
    SelfHarmInstructions,
    Sexual,
    SexualMinors,
    Violence,
    ViolenceGraphic,
}

impl ModerationCategory {
    pub fn description(&self) -> &'static str {
        match self {
            ModerationCategory::Hate => "Content that expresses, incites, or promotes hate based on race, gender, ethnicity, religion, nationality, sexual orientation, disability status, or caste. Hateful content aimed at non-protected groups (e.g., chess players) is harassment.",
            ModerationCategory::HateThreatening => "Hateful content that also includes violence or serious harm towards the targeted group based on race, gender, ethnicity, religion, nationality, sexual orientation, disability status, or caste.",
            ModerationCategory::Harassment => "Content that expresses, incites, or promotes harassing language towards any target.",
            ModerationCategory::HarassmentThreatening => "Harassment content that also includes violence or serious harm towards any target.",
            ModerationCategory::SelfHarm => "Content that promotes, encourages, or depicts acts of self-harm, such as suicide, cutting, and eating disorders.",
            ModerationCategory::SelfHarmIntent => "Content where the speaker expresses that they are engaging or intend to engage in acts of self-harm, such as suicide, cutting, and eating disorders.",
            ModerationCategory::SelfHarmInstructions => "Content that encourages performing acts of self-harm, such as suicide, cutting, and eating disorders, or that gives instructions or advice on how to commit such acts.",
            ModerationCategory::Sexual => "Content meant to arouse sexual excitement, such as the description of sexual activity, or that promotes sexual services (excluding sex education and wellness).",
            ModerationCategory::SexualMinors => "Sexual content that includes an individual who is under 18 years old.",
            ModerationCategory::Violence => "Content that depicts death, violence, or physical injury.",
            ModerationCategory::ViolenceGraphic => "Content that depicts death, violence, or physical injury in graphic detail.",
        }
    }

    pub fn nip56_report_type(&self) -> Report {
        match self {
            ModerationCategory::Hate
            | ModerationCategory::HateThreatening
            | ModerationCategory::Harassment
            | ModerationCategory::HarassmentThreatening
            | ModerationCategory::SelfHarm
            | ModerationCategory::SelfHarmIntent
            | ModerationCategory::SelfHarmInstructions
            | ModerationCategory::Violence
            | ModerationCategory::ViolenceGraphic => Report::Other,

            ModerationCategory::Sexual => Report::Nudity,

            ModerationCategory::SexualMinors => Report::Illegal,
        }
    }

    pub fn nip69(&self) -> &'static str {
        match self {
            ModerationCategory::Hate => "IH",
            ModerationCategory::HateThreatening => "HC-bhd",
            ModerationCategory::Harassment => "IL-har",
            ModerationCategory::HarassmentThreatening => "HC-bhd",
            ModerationCategory::SelfHarm => "HC-bhd",
            ModerationCategory::SelfHarmIntent => "HC-bhd",
            ModerationCategory::SelfHarmInstructions => "HC-bhd",
            ModerationCategory::Sexual => "NS",
            ModerationCategory::SexualMinors => "IL-csa",
            ModerationCategory::Violence => "VI",
            ModerationCategory::ViolenceGraphic => "VI",
        }
    }
}

impl FromStr for ModerationCategory {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "hate" => Ok(ModerationCategory::Hate),
            "hate/threatening" => Ok(ModerationCategory::HateThreatening),
            "harassment" => Ok(ModerationCategory::Harassment),
            "harassment/threatening" => Ok(ModerationCategory::HarassmentThreatening),
            "self-harm" => Ok(ModerationCategory::SelfHarm),
            "self-harm/intent" => Ok(ModerationCategory::SelfHarmIntent),
            "self-harm/instructions" => Ok(ModerationCategory::SelfHarmInstructions),
            "sexual" => Ok(ModerationCategory::Sexual),
            "sexual/minors" => Ok(ModerationCategory::SexualMinors),
            "violence" => Ok(ModerationCategory::Violence),
            "violence/graphic" => Ok(ModerationCategory::ViolenceGraphic),
            _ => Err(anyhow!("Invalid moderation category {}", input)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!(
            ModerationCategory::from_str("hate").unwrap(),
            ModerationCategory::Hate
        );
        assert_eq!(
            ModerationCategory::from_str("harassment").unwrap(),
            ModerationCategory::Harassment
        );

        assert!(ModerationCategory::from_str("non-existent").is_err());
    }

    #[test]
    fn test_description() {
        let hate = ModerationCategory::Hate;
        assert_eq!(hate.description(), "Content that expresses, incites, or promotes hate based on race, gender, ethnicity, religion, nationality, sexual orientation, disability status, or caste. Hateful content aimed at non-protected groups (e.g., chess players) is harassment.");
    }

    #[test]
    fn test_nip56_report_type() {
        let harassment = ModerationCategory::Harassment;
        assert_eq!(harassment.nip56_report_type(), Report::Other);
    }

    #[test]
    fn test_nip69() {
        let violence = ModerationCategory::Violence;
        assert_eq!(violence.nip69(), "VI");
    }
}
