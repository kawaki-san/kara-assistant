use std::thread;

use crate::Config;

use self::processing::{convert_buffer, merge_buffers};

pub mod processing;
#[derive(Debug, Clone)]
pub enum Event {
    RequestData(crossbeam_channel::Sender<Vec<f32>>),
    SendData(Vec<f32>),
    RequestConfig(crossbeam_channel::Sender<Config>),
    SendConfig(Config),
    RequestRefresh,
    ClearBuffer,
}
pub struct AudioStream {
    event_sender: crossbeam_channel::Sender<Event>,
}
impl AudioStream {
    pub fn new(config: &Config) -> Self {
        let (event_sender, event_receiver) = crossbeam_channel::unbounded();
        let inner_config = config.clone();

        let refresh_rate = config.refresh_rate;
        // thread that receives Events, converts and processes the received data
        // and sends it via a crossbeam_channel channel to requesting to thread that requested processed data
        tokio::spawn(async move {
            //let (event_sender, event_receiver) = crossbeam_channel::channel();
            let mut buffer: Vec<f32> = Vec::new();
            let mut calculated_buffer: Vec<f32> = Vec::new();
            let mut smoothing_buffer: Vec<Vec<f32>> = Vec::new();
            let mut smoothed_buffer: Vec<f32> = Vec::new();
            let mut config: Config = inner_config.clone();

            loop {
                match event_receiver.recv().unwrap() {
                    Event::SendData(mut b) => {
                        buffer.append(&mut b);
                        let resolution = config.resolution.into();
                        while buffer.len() > resolution {
                            let c_b = convert_buffer(&buffer[0..resolution].to_vec(), &config);

                            calculated_buffer = if !calculated_buffer.is_empty() {
                                merge_buffers(&vec![calculated_buffer, c_b])
                            } else {
                                c_b
                            };
                            // remove already calculated parts
                            buffer.drain(0..resolution);
                        }
                    }
                    Event::RequestData(sender) => {
                        sender
                            .send(smoothed_buffer.clone())
                            .expect("audio thread lost connection to bridge");
                    }
                    Event::RequestRefresh => {
                        if !calculated_buffer.is_empty() {
                            smoothing_buffer.push(calculated_buffer.clone());
                        }
                        smoothed_buffer = if !smoothing_buffer.is_empty() {
                            merge_buffers(&smoothing_buffer)
                        } else {
                            Vec::new()
                        };
                        while smoothing_buffer.len() > config.buffering.into() {
                            smoothing_buffer.remove(0);
                        }
                    }
                    Event::RequestConfig(sender) => {
                        sender.send(config.clone()).unwrap();
                    }
                    Event::SendConfig(c) => {
                        config = c;
                    }
                    Event::ClearBuffer => {
                        calculated_buffer = Vec::new();
                    }
                }
            }
        });
        let event_sender_clone = event_sender.clone();
        tokio::spawn(async move {
            loop {
                thread::sleep(std::time::Duration::from_millis(1000 / refresh_rate as u64));
                event_sender_clone.send(Event::RequestRefresh).unwrap();
            }
        });

        AudioStream { event_sender }
    }
    pub fn get_audio_data(&self) -> Vec<f32> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.event_sender.send(Event::RequestData(tx)).unwrap();
        rx.recv().unwrap()
    }
    pub fn get_event_sender(&self) -> crossbeam_channel::Sender<Event> {
        self.event_sender.clone()
    }

    // modifying the amount of bars during runtime will result in unexpected behavior
    // unless sending 'Event::ClearBuffer' before
    // because the converter assumes that the bar amount stays the same
    // could be fixed by modifying ./src/processing/combine_buffers
    pub fn set_config(&self, config: Config) {
        self.event_sender.send(Event::SendConfig(config)).unwrap();
    }
    pub fn get_config(&self) -> Config {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.event_sender.send(Event::RequestConfig(tx)).unwrap();
        rx.recv().unwrap()
    }
}
