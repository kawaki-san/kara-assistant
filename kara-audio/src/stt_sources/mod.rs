use std::fs::create_dir_all;

use serde::Deserialize;

use self::kara::{init_kara_model, KaraTranscriber};

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

impl Default for STTConfig {
    fn default() -> Self {
        Self::Kara(default_stt_model_path())
    }
}

pub fn default_stt_model_path() -> String {
    let mut dir = dirs::data_dir().expect("could not find data dir");
    dir.push("kara");
    dir.push("stt");
    create_dir_all(&dir).unwrap();
    dir.display().to_string()
}

// Store coqui on all variants as fallback?
#[derive(Clone)]
pub enum STTSource {
    Kara(KaraTranscriber),
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
