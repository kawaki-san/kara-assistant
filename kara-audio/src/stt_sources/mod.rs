use serde::Deserialize;

use self::kara::{init_kara_model, KaraTranscriber};

pub mod kara;

/// Store the configurations/credentials for all the services that
/// provide STT
#[derive(Debug, Deserialize)]
pub enum STTConfig {
    Kara(String, u32, bool),
    Gcp,
    Watson,
}

impl STTConfig {
    pub fn base(path: &str, silence_level: u32, show_amp: bool) -> Self {
        STTConfig::Kara(path.to_owned(), silence_level, show_amp)
    }
}

impl Default for STTConfig {
    fn default() -> Self {
        Self::Kara(String::default(), 150, false)
    }
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
        STTConfig::Kara(model, silence_level, show_amp) => {
            init_kara_model(model, silence_level, show_amp)
                .await
                .map_err(anyhow::Error::msg)
        }
        STTConfig::Gcp => todo!(),
        STTConfig::Watson => todo!(),
    }
}
