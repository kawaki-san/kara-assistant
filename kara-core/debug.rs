use std::path::{Path, PathBuf};

use kara_audio::crossbeam_channel;
use tracing::{info, trace};
use tracing_subscriber::{
    filter, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt,
};

use crate::config::{state::ParsedConfig, ConfigFile};

pub fn initialise() -> (
    tracing_appender::non_blocking::WorkerGuard,
    ParsedConfig,
    crossbeam_channel::Receiver<kara_nlu::NLUParser>,
) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    tokio::spawn(async move {
        let engine = kara_nlu::NLUParser::new("kara-assets/nlu");
        tx.send(engine).unwrap();
    });
    let args = crate::cli::initialise();
    let config: ConfigFile = if let Some(file) = args.config_path() {
        match std::fs::read_to_string(file) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(conf) => conf,
                Err(err) => {
                    // could not parse specified file. use @1
                    eprintln!("{}", err);
                    config_path_1()
                }
            },
            Err(err) => {
                eprintln!("{}", err);
                config_path_1()
            }
        }
    } else {
        config_path_1()
    };
    let mut config: ParsedConfig = ParsedConfig::from(config);
    config.general_settings.startup_mode = args.interface(config.general_settings.startup_mode);
    let filter =
        filter::Targets::new().with_target("kara", args.debug(config.general_settings.log_level));
    let file_appender = tracing_appender::rolling::daily(log_dir(), "kara.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .with(
            tracing_subscriber::fmt::layer(), //.pretty()
        )
        .with(filter)
        .init();
    trace!(
        "starting in {} mode",
        args.interface(config.general_settings.startup_mode)
            .to_string()
    );
    info!(
        "{} {} has started",
        env!("CARGO_BIN_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    (guard, config, rx)
}

fn config_path_1() -> ConfigFile {
    let mut config = config_dir();
    config.push("kara");
    config.push("kara.toml");
    match std::fs::read_to_string(&config) {
        Ok(val) => match toml::from_str(&val) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("{err}");
                config_path_2()
            }
        },
        Err(err) => {
            read_file_err(&err);
            config_path_2()
        }
    }
}

fn read_file_err(err: &std::io::Error) {
    let kind = err.kind();
    match kind {
        std::io::ErrorKind::NotFound => {}
        _ => eprintln!("{err}"),
    }
}

fn config_path_2() -> ConfigFile {
    let mut config = config_dir();
    config.push("kara.toml");
    match std::fs::read_to_string(&config) {
        Ok(val) => match toml::from_str(&val) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("{err}");
                config_default()
            }
        },
        Err(err) => {
            read_file_err(&err);
            config_default()
        }
    }
}

fn config_default() -> ConfigFile {
    let contents = include_str!("../kara.toml");
    toml::from_str(contents).expect("parsing config file")
}

fn log_dir() -> impl AsRef<Path> {
    let mut cache_dir = dirs::cache_dir().expect("could not find cache dir");
    cache_dir.push("kara");
    cache_dir
}

fn config_dir() -> PathBuf {
    dirs::config_dir().expect("could not find config dir")
}
