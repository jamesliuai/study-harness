use serde::Serialize;
#[derive(Clone, Serialize)]
pub struct ProviderRegistry {
    pub version: &'static str,
    pub provider: &'static str,
    pub transcription_model: &'static str,
    pub endpoint: &'static str,
    pub audio_microusd_per_second: f64,
}
pub const OPENAI: ProviderRegistry = ProviderRegistry {
    version: "openai-standard-2026-10-29-v2",
    provider: "OpenAI",
    transcription_model: "gpt-transcribe",
    endpoint: "https://api.openai.com/v1",
    audio_microusd_per_second: 75.,
};

/// A tutor model the learner may choose. Rates are standard-tier, short-context list prices (USD per 1M tokens equals
/// micro-USD per token), checked against the official pricing page on 2026-10-29; reasoning tokens bill as output.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TutorModel {
    pub id: &'static str,
    pub label: &'static str,
    pub note: &'static str,
    pub input_microusd_per_token: f64,
    pub output_microusd_per_token: f64,
}
pub const TUTOR_MODELS: [TutorModel; 3] = [
    TutorModel {
        id: "gpt-6-astra",
        label: "Astra",
        note: "Most capable · 5× Sol's price",
        input_microusd_per_token: 10.,
        output_microusd_per_token: 50.,
    },
    TutorModel {
        id: "gpt-6.1-sol",
        label: "Sol",
        note: "Near-Astra quality · recommended",
        input_microusd_per_token: 2.,
        output_microusd_per_token: 10.,
    },
    TutorModel {
        id: "gpt-6.1-terra",
        label: "Terra",
        note: "Fast and efficient · half Sol's price",
        input_microusd_per_token: 1.,
        output_microusd_per_token: 5.,
    },
];
pub const DEFAULT_TUTOR_MODEL: &str = "gpt-6.1-sol";
/// Reasoning effort, lowest first. "low" is the tutor default the app used before the choice existed.
pub const EFFORTS: [&str; 3] = ["low", "medium", "high"];
pub const DEFAULT_EFFORT: &str = "low";

pub fn tutor_model(id: &str) -> Option<&'static TutorModel> {
    TUTOR_MODELS.iter().find(|m| m.id == id)
}
fn effort(name: &str) -> Option<&'static str> {
    EFFORTS.iter().copied().find(|e| *e == name)
}
pub fn valid_model(id: &str) -> bool {
    tutor_model(id).is_some()
}
pub fn valid_effort(name: &str) -> bool {
    effort(name).is_some()
}

/// What a request is for; plans and tasks need a correct answer key, so they never think below medium.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Purpose {
    Tutor,
    Plan,
    Task,
}
impl Purpose {
    pub fn of(kind: &str) -> Option<Purpose> {
        match kind.trim_start_matches("offline:") {
            "tutor" => Some(Purpose::Tutor),
            "plan" => Some(Purpose::Plan),
            "task" => Some(Purpose::Task),
            _ => None,
        }
    }
}

/// The model and reasoning effort frozen for one request when it is prepared. A request in flight keeps it even if
/// the learner changes the preference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelChoice {
    pub model: &'static TutorModel,
    pub effort: &'static str,
}
impl Default for ModelChoice {
    fn default() -> Self {
        Self::resolve(DEFAULT_TUTOR_MODEL, DEFAULT_EFFORT)
    }
}
impl ModelChoice {
    /// Unknown or empty values (records from before the choice existed) fall back to Sol at low effort, which is
    /// exactly what those requests used.
    pub fn resolve(model: &str, effort_name: &str) -> Self {
        Self {
            model: tutor_model(model)
                .or_else(|| tutor_model(DEFAULT_TUTOR_MODEL))
                .expect("default tutor model is registered"),
            effort: effort(effort_name).unwrap_or(DEFAULT_EFFORT),
        }
    }
    /// The effort actually sent: the choice for tutoring, at least medium for plans and tasks.
    pub fn effort_for(&self, purpose: Purpose) -> &'static str {
        if purpose == Purpose::Tutor || self.effort == "high" {
            self.effort
        } else {
            "medium"
        }
    }
    /// Output allowance (reasoning included) scaled with effort so deeper thinking is not cut off mid-answer.
    pub fn max_output(&self, purpose: Purpose) -> u32 {
        match (purpose, self.effort_for(purpose)) {
            (Purpose::Tutor, "low") => 4096,
            (Purpose::Tutor, "medium") => 8192,
            (Purpose::Tutor, _) => 16384,
            (Purpose::Plan, "high") => 12000,
            (Purpose::Plan, _) => 6000,
            (Purpose::Task, "high") => 18000,
            (Purpose::Task, _) => 9000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_choice_defaults_floors_and_scales() {
        let legacy = ModelChoice::resolve("", "");
        assert_eq!(legacy, ModelChoice::default());
        assert_eq!(legacy.model.id, "gpt-6.1-sol");
        assert_eq!(legacy.effort_for(Purpose::Tutor), "low");
        assert_eq!(legacy.max_output(Purpose::Tutor), 4096);
        // Plans and tasks never go below medium; their allowances match the earlier fixed values.
        assert_eq!(legacy.effort_for(Purpose::Plan), "medium");
        assert_eq!(legacy.max_output(Purpose::Plan), 6000);
        assert_eq!(legacy.max_output(Purpose::Task), 9000);
        let deep = ModelChoice::resolve("gpt-6-astra", "high");
        assert_eq!(deep.model.label, "Astra");
        assert_eq!(deep.effort_for(Purpose::Task), "high");
        assert_eq!(deep.max_output(Purpose::Tutor), 16384);
        assert_eq!(deep.max_output(Purpose::Plan), 12000);
        assert_eq!(deep.max_output(Purpose::Task), 18000);
        let medium = ModelChoice::resolve("gpt-6.1-terra", "medium");
        assert_eq!(medium.max_output(Purpose::Tutor), 8192);
        assert_eq!(medium.effort_for(Purpose::Plan), "medium");
        assert!(!valid_model("gpt-4o") && !valid_effort("max"));
        assert_eq!(Purpose::of("offline:task"), Some(Purpose::Task));
        assert_eq!(Purpose::of("transcription"), None);
    }
}
