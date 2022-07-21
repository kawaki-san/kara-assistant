mod cli;
mod config;
mod debug;
mod gui;

#[tokio::main]
async fn main() {
    let (_guard, config, model_receiver) = debug::initialise();

    match config.general_settings.startup_mode {
        cli::Interface::Cli => {
            println!("Hello, world!");
        }
        cli::Interface::Gui => {
            if let Err(e) = gui::start(&config, model_receiver).await {
                tracing::error!("{}", e);
            }
        }
    }
}
