//! Timeout configuration

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeout {
    #[serde(default)]
    pub after: Option<TimeoutDuration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutDuration {
    #[serde(default)]
    pub seconds: Option<u64>,
    #[serde(default)]
    pub minutes: Option<u64>,
    #[serde(default)]
    pub hours: Option<u64>,
}

impl Timeout {
    /// The time allowed, if one is set.
    pub fn duration(&self) -> Option<std::time::Duration> {
        let a = self.after.as_ref()?;
        let secs = a.seconds.unwrap_or(0) + a.minutes.unwrap_or(0) * 60 + a.hours.unwrap_or(0) * 3600;
        (secs > 0).then(|| std::time::Duration::from_secs(secs))
    }
}
