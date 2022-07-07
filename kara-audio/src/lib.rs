use std::{
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Instant,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample,
};
use crossbeam_channel::{Receiver, Sender};
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
    is_processing: Arc<Mutex<AtomicBool>>,
    tx: Sender<Vec<f32>>,
    rx: Receiver<Vec<f32>>,
    wake_up: Arc<Mutex<AtomicBool>>,
) -> mpsc::Sender<Event> {
    let audio_stream = AudioStream::init(&vis_settings);
    let event_sender = audio_stream.get_event_sender();
    init_audio_sender(
        event_sender.clone(),
        stt_proxy,
        stt_source,
        is_processing,
        tx,
        rx,
        wake_up,
    );
    event_sender
}

pub fn init_audio_sender(
    event_sender: mpsc::Sender<Event>,
    stt_proxy: EventLoopProxy<KaraEvents>,
    stt_source: STTSource,
    is_processing: Arc<Mutex<AtomicBool>>,
    tx: Sender<Vec<f32>>,
    rx: Receiver<Vec<f32>>,
    wake_up: Arc<Mutex<AtomicBool>>,
) {
    let inner_is_processing = Arc::clone(&is_processing);
    let inner_wake = Arc::clone(&wake_up);
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
                    send_to_visualiser(s, event_sender.clone());
                    // Not currently processing any command, so do transcription
                    let is_processing = inner_is_processing.lock().unwrap();
                    let is_awake = inner_wake.lock().unwrap();
                    // TODO: check if wake word here
                    if !is_processing.load(Ordering::Relaxed) && is_awake.load(Ordering::Relaxed) {
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
    tokio::spawn(async move {
        loop {
            if !is_processing.lock().unwrap().load(Ordering::Relaxed)
                && wake_up.lock().unwrap().load(Ordering::Relaxed)
            {
                match stt_source.clone() {
                    STTSource::Kara(model) => {
                        let stream = Arc::clone(&model);
                        let mut stream = stream.lock().unwrap();
                        let mut silence_start: Option<Instant> = None;
                        let mut sound_from_start_till_pause: Vec<f32> = Vec::new();
                        while let Ok(val) = rx.clone().recv() {
                            sound_from_start_till_pause.extend(&val);
                            let sound_as_ints = val.iter().map(|f| (*f * 1000.0) as i32);
                            let max_amplitude = sound_as_ints.clone().max().unwrap_or(0);
                            let min_amplitude = sound_as_ints.clone().min().unwrap_or(0);
                            let silence_detected = max_amplitude < 200 && min_amplitude > -200;
                            if silence_detected {
                                match silence_start {
                                    Some(s) => {
                                        if s.elapsed().as_secs_f32() > 1.5 {
                                            break;
                                        }
                                    }
                                    None => silence_start = Some(Instant::now()),
                                }
                            } else {
                                silence_start = None;
                            }
                            let val = val.iter().map(|f| f.to_i16()).collect::<Vec<_>>();
                            stream.accept_waveform(&val);
                            if let Err(e) = prox.send_event(KaraEvents::SpeechFeed(
                                stream.partial_result().partial.to_owned(),
                            )) {
                                error!("{}", e);
                            }
                        }
                        let results = stream.final_result().single().unwrap().text.to_owned();
                        if let Err(e) = stt_proxy.send_event(KaraEvents::ProcessCommand(results)) {
                            error!("{e}");
                        } else {
                            is_processing.lock().unwrap().store(true, Ordering::Relaxed);
                            tracing::trace!("channel");
                        }
                    }
                    STTSource::Gcp => todo!(),
                    STTSource::Watson => todo!(),
                }
            }
        }
    });
}
fn send_to_visualiser(data: &[f32], sender: mpsc::Sender<Event>) {
    // sends the raw data to audio_stream via the event_sender
    sender.send(Event::SendData(data.to_vec())).unwrap();
}

fn err_fn(err: cpal::StreamError) {
    error!("an error occurred on stream: {}", err);
}
