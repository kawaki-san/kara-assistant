use std::sync::{Arc, Mutex};

use serde::Deserialize;

use self::kara::init_kara_model;

pub mod kara;

/// Store the configurations/credentials for all the services that
/// provide STT
#[derive(Debug, Deserialize)]
pub enum STTConfig {
    Kara(String),
    Gcp,
    Watson,
}

impl STTConfig {
    pub fn base(path: &str) -> Self {
        STTConfig::Kara(path.to_owned())
    }
}

// Store coqui on all variants as fallback?
#[derive(Clone)]
pub enum STTSource {
    Kara(Arc<Mutex<vosk::Recognizer>>),
    Gcp,
    Watson,
}

#[tracing::instrument]
pub async fn stt_source(source: &STTConfig) -> anyhow::Result<STTSource> {
    match source {
        STTConfig::Kara(model) => init_kara_model(model).await.map_err(anyhow::Error::msg),
        STTConfig::Gcp => todo!(),
        STTConfig::Watson => todo!(),
    }
}
