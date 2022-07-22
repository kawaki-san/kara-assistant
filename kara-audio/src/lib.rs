use std::{
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample,
};
use iced_winit::winit::event_loop::EventLoopProxy;
use tracing::{debug, error};

use self::{
    events::KaraEvents,
    stream::{AudioStream, Event},
    stt_sources::STTSource,
};

pub mod events;
pub mod stream;
pub mod stt_sources;
pub const SAMPLE_RATE: u32 = 16000;

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
    stt_proxy: EventLoopProxy<KaraEvents>,
    stt_source: STTSource,
    is_processing: Arc<AtomicBool>,
    wake_up: Arc<AtomicBool>,
    is_ready: Arc<AtomicBool>,
) {
    let inner_is_processing = Arc::clone(&is_processing);
    let inner_is_ready = Arc::clone(&is_ready);
    let (tx, rx) = crossbeam_channel::unbounded();
    tokio::spawn(async move {
        let host = cpal::default_host();
        // Set up the input device and stream with the default input config.
        let device = host.default_input_device().unwrap();
        debug!("using audio device ({})", device.name().unwrap());

        let mut config = device.default_input_config().unwrap();
        if config.channels() != 1 {
            let mut supported_configs_range = device.supported_input_configs().unwrap();
            config = match supported_configs_range.next() {
                Some(conf) => {
                    conf.with_sample_rate(cpal::SampleRate(SAMPLE_RATE)) //16K from deepspeech
                }
                None => config,
            };
        }
        let channels = config.channels();
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data, _: &_| {
                    use rubato::{
                        InterpolationParameters, InterpolationType, Resampler, SincFixedIn,
                        WindowFunction,
                    };
                    let params = InterpolationParameters {
                        sinc_len: 256,
                        f_cutoff: 0.95,
                        interpolation: InterpolationType::Linear,
                        oversampling_factor: 256,
                        window: WindowFunction::BlackmanHarris2,
                    };
                    let mut resampler = SincFixedIn::<f32>::new(
                        44100_f64 / SAMPLE_RATE as f64,
                        3.0,
                        params,
                        data.len(),
                        channels.into(),
                    )
                    .unwrap();

                    let waves_in = vec![data];
                    let waves_out = resampler.process(&waves_in, None).unwrap();
                    let s = waves_out.first().unwrap();
                    // Not currently processing any command, so do transcription
                    // TODO: check if wake word here
                    if !inner_is_processing.load(Ordering::Relaxed)
                        && inner_is_ready.load(Ordering::Relaxed)
                    {
                        send_to_visualiser(s, event_sender.clone());
                        tx.send(data.to_owned()).unwrap();
                    }
                },
                err_fn,
            )
            .unwrap();
        stream.play().unwrap();
        // parks the thread so stream.play() does not get dropped and stops
        thread::park();
    });
    // If we're not processing, spawn a new thread for transcription
    let prox = stt_proxy.clone();
    let is_processing = Arc::clone(&is_processing);

    let inner_wake = Arc::clone(&wake_up);
    tokio::spawn(async move {
        loop {
            if !is_processing.load(Ordering::Relaxed) && is_ready.load(Ordering::Relaxed) {
                match &stt_source {
                    STTSource::Kara(kara_transcriber) => {
                        let stream = if inner_wake.load(Ordering::Relaxed) {
                            Arc::clone(&kara_transcriber.recogniser())
                        } else {
                            Arc::clone(&kara_transcriber.wake_word_recogniser())
                        };
                        let mut recogniser = stream.lock().unwrap();
                        if let Ok(val) = rx.clone().recv() {
                            let val = val.iter().map(|f| f.to_i16()).collect::<Vec<_>>();
                            let stream = recogniser.accept_waveform(&val);
                            let result = recogniser.partial_result().partial;
                            match stream {
                                vosk::DecodingState::Finalized => {
                                    if inner_wake.load(Ordering::Relaxed) {
                                        let results = recogniser
                                            .final_result()
                                            .single()
                                            .unwrap()
                                            .text
                                            .to_owned();
                                        if let Err(e) = stt_proxy
                                            .send_event(KaraEvents::ProcessCommand(results))
                                        {
                                            error!("{e}");
                                        } else {
                                            is_processing.store(true, Ordering::Relaxed);
                                            tracing::trace!("channel");
                                        }
                                    } else {
                                        inner_wake.store(true, Ordering::Relaxed);
                                        recogniser.reset();
                                    }
                                }
                                vosk::DecodingState::Running => {
                                    if inner_wake.load(Ordering::Relaxed) {
                                        if let Err(e) = prox
                                            .send_event(KaraEvents::SpeechFeed(result.to_owned()))
                                        {
                                            error!("{}", e);
                                        }
                                    }
                                    if result.eq_ignore_ascii_case("[unk]") {
                                        recogniser.reset();
                                    }
                                }
                                vosk::DecodingState::Failed => todo!(),
                            }
                        }
                    }
                    STTSource::Gcp => todo!(),
                    STTSource::Watson => todo!(),
                }
            }
        }
    });
}
fn send_to_visualiser(data: &[f32], sender: crossbeam_channel::Sender<Event>) {
    // sends the raw data to audio_stream via the event_sender
    sender.send(Event::SendData(data.to_vec())).unwrap();
}

fn err_fn(err: cpal::StreamError) {
    error!("an error occurred on stream: {}", err);
}

pub use crossbeam_channel;
