use std::{
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Sender;
use dasp::{sample::ToSample, Sample};
use iced_winit::winit::event_loop::EventLoopProxy;
use tracing::{debug, error};

use self::{
    events::KaraEvents,
    helpers::set_sample_rate,
    stream::{AudioStream, Event},
    stt_sources::STTSource,
};

mod helpers;

pub mod events;
pub mod stream;
pub mod stt_sources;
pub const SAMPLE_RATE: u32 = 16000;

#[derive(Debug)]
pub(crate) struct StreamDevice {
    channel_count: u8,
    sample_rate: u32,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub buffering: usize,
    pub smoothing_size: usize,
    pub smoothing_amount: usize,
    pub resolution: usize,
    pub refresh_rate: usize,
    pub frequency_scale_range: Range<usize>,
    pub frequency_scale_amount: usize,
    pub density_reduction: usize,
    pub max_frequency: usize,
    pub volume: f32,
}
impl Default for Config {
    fn default() -> Self {
        Config {
            buffering: 5,
            smoothing_size: 10,
            smoothing_amount: 5,
            resolution: 3000,
            refresh_rate: 60,
            frequency_scale_range: Range {
                start: 50,
                end: 1000,
            },
            frequency_scale_amount: 1,
            density_reduction: 5,
            //max_frequency: 20_000,
            max_frequency: 12_500,
            volume: 3.2,
        }
    }
}

pub fn start_stream(
    vis_settings: Config,
    stt_proxy: EventLoopProxy<KaraEvents>,
    stt_source: STTSource,
    is_processing: Arc<AtomicBool>,
    wake_up: Arc<AtomicBool>,
    is_ready: Arc<AtomicBool>,
) -> crossbeam_channel::Sender<Event> {
    let audio_stream = AudioStream::new(&vis_settings);
    let event_sender = audio_stream.get_event_sender();
    init_audio_sender(
        event_sender.clone(),
        stt_proxy,
        stt_source,
        is_processing,
        wake_up,
        is_ready,
    );
    event_sender
}

pub fn init_audio_sender(
    event_sender: crossbeam_channel::Sender<Event>,
    event_proxy: EventLoopProxy<KaraEvents>,
    stt_source: STTSource,
    is_processing: Arc<AtomicBool>,
    wake_up: Arc<AtomicBool>,
    is_ready: Arc<AtomicBool>,
) {
    let inner_is_processing = Arc::clone(&is_processing);
    let inner_is_ready = Arc::clone(&is_ready);
    let (tx, rx) = crossbeam_channel::unbounded();

    let err_fn = move |err| {
        eprintln!("an error occurred on stream: {}", err);
    };

    tokio::spawn(async move {
        let host = cpal::default_host();
        // Set up the input device and stream with the default input config.
        let device = host.default_input_device().unwrap();
        debug!("using audio device ({})", device.name().unwrap());

        let config = device.default_input_config().unwrap();
        let sample_rate = config.sample_rate().0;
        let stream_device = StreamDevice {
            channel_count: config.channels() as u8,
            sample_rate,
        };
        debug!(
            "using audio device ({}) with: {:#?}",
            device.name().unwrap(),
            stream_device
        );
        let stream = match config.sample_format() {
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    resample(
                        data,
                        &stream_device,
                        tx.clone(),
                        event_sender.clone(),
                        Arc::clone(&inner_is_processing),
                        Arc::clone(&inner_is_ready),
                    )
                },
                err_fn,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _| {
                    resample(
                        data,
                        &stream_device,
                        tx.clone(),
                        event_sender.clone(),
                        Arc::clone(&inner_is_processing),
                        Arc::clone(&inner_is_ready),
                    )
                },
                err_fn,
            ),
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    resample(
                        data,
                        &stream_device,
                        tx.clone(),
                        event_sender.clone(),
                        Arc::clone(&inner_is_processing),
                        Arc::clone(&inner_is_ready),
                    )
                },
                err_fn,
            ),
        }
        .unwrap();
        stream.play().unwrap();
        // parks the thread so stream.play() does not get dropped and stops
        thread::park();
    });
    // If we're not processing, spawn a new thread for transcription
    let is_processing = Arc::clone(&is_processing);

    let inner_wake = Arc::clone(&wake_up);
    tokio::spawn(async move {
        loop {
            if !is_processing.load(Ordering::Relaxed) && is_ready.load(Ordering::Relaxed) {
                match &stt_source {
                    STTSource::Kara(kara_transcriber) => {
                        /*
                        let stream = if inner_wake.load(Ordering::Relaxed) {
                            kara_transcriber.recogniser()
                        } else {
                            todo!("wake word listener");
                        };
                        */
                        let stream = kara_transcriber.recogniser();
                        let mut recogniser = stream.lock().unwrap();
                        while let Ok(val) = rx.clone().recv() {
                            let stream = recogniser.accept_waveform(&val);
                            match stream {
                                vosk::DecodingState::Finalized => {
                                    break;
                                }
                                vosk::DecodingState::Running => {
                                    if let Err(e) = event_proxy.send_event(KaraEvents::SpeechFeed(
                                        recogniser.partial_result().partial.to_owned(),
                                    )) {
                                        error!("{}", e);
                                    }
                                }
                                vosk::DecodingState::Failed => todo!(),
                            }
                        }

                        if inner_wake.load(Ordering::Relaxed) {
                            // We're awake so process command
                            if let Err(e) = event_proxy.send_event(KaraEvents::ProcessCommand(
                                recogniser.result().single().unwrap().text.to_owned(),
                            )) {
                                error!("{e}");
                            };
                        } else {
                            // check if wake word and send wake up event
                        }
                    }
                    STTSource::Gcp => todo!(),
                    STTSource::Watson => todo!(),
                }
            }
        }
    });
}
fn send_to_visualiser(data: Vec<f32>, sender: crossbeam_channel::Sender<Event>) {
    // sends the raw data to audio_stream via the event_sender
    sender.send(Event::SendData(data)).unwrap();
}

fn resample(
    data: &[impl Sample + ToSample<f32>],
    stream_device: &StreamDevice,
    transcription_sender: Sender<Vec<i16>>,
    event_sender: Sender<Event>,
    is_processing: Arc<AtomicBool>,
    is_ready: Arc<AtomicBool>,
) {
    // convert 44100 to 16000
    // convert stereo to mono
    if !is_ready.load(Ordering::Relaxed) {
        // if kara is still getting ready, write a constant on the vis
        let silence = write_silence(data);
        send_to_visualiser(silence, event_sender);
    } else if !is_processing.load(Ordering::Relaxed) && is_ready.load(Ordering::Relaxed) {
        let audio_vis: Vec<_> = data.iter().map(|f| f.to_sample::<f32>()).collect();
        // resample samples
        let adjusted_feed = set_sample_rate(&audio_vis, stream_device);
        let transcription_feed = if stream_device.channel_count != 1 {
            helpers::stereo_to_mono(&adjusted_feed)
        } else {
            adjusted_feed
        };

        send_to_visualiser(audio_vis, event_sender);
        transcription_sender.send(transcription_feed).unwrap();
    }
}

fn write_silence(data: &[impl Sample + ToSample<f32>]) -> Vec<f32> {
    let mut data: Vec<_> = data.to_owned();
    let data = data.iter_mut().map(|_| 0.01.to_sample::<f32>()).collect();
    data
}

pub use crossbeam_channel;
