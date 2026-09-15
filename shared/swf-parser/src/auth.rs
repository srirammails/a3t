//! Authentication schemes

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Authentication {
    Basic { basic: BasicAuth },
    Bearer { bearer: String },
    Certificate { certificate: CertAuth },
    Digest { digest: DigestAuth },
    OAuth2 { oauth2: OAuth2Config },
    OpenIdConnect { openIdConnect: OIDCConfig },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertAuth {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestAuth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    pub authority: String,
    pub grant: String,
    #[serde(default)]
    pub client: Option<OAuth2Client>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub audiences: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Client {
    pub id: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OIDCConfig {
    pub authority: String,
    #[serde(default)]
    pub grant: Option<String>,
}
