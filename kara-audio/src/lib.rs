use std::{
    ops::Range,
    sync::{mpsc, Arc},
    thread,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample,
};
use iced_winit::winit::event_loop::EventLoopProxy;
use tracing::{debug, error};

use self::{
    stream::{AudioStream, Event},
    stt_sources::STTSource,
};

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

pub fn visualiser_stream(
    vis_settings: Config,
    stt_proxy: EventLoopProxy<String>,
    stt_source: STTSource,
) -> mpsc::Sender<Event> {
    let audio_stream = AudioStream::init(&vis_settings);
    let event_sender = audio_stream.get_event_sender();
    init_audio_sender(event_sender.clone(), stt_proxy, stt_source);
    event_sender
}

pub fn init_audio_sender(
    event_sender: mpsc::Sender<Event>,
    stt_proxy: EventLoopProxy<String>,
    stt_source: STTSource,
) {
    let (tx, rx) = mpsc::channel();
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
                    tx.send(data.to_owned()).unwrap();
                },
                err_fn,
            )
            .unwrap();
        stream.play().unwrap();
        // parks the thread so stream.play() does not get dropped and stops
        thread::park();
    });
    tokio::spawn(async move {
        match stt_source {
            STTSource::Kara(model) => {
                let stream = Arc::clone(&model);
                let mut stream = stream.lock().unwrap();
                while let Ok(val) = rx.recv() {
                    let val = val.iter().map(|f| f.to_i16()).collect::<Vec<_>>();
                    stream.accept_waveform(&val);
                    if let Err(e) = stt_proxy.send_event(stream.partial_result().partial.to_owned())
                    {
                        tracing::error!("{}", e);
                    }
                }
            }
            STTSource::Gcp => todo!(),
            STTSource::Watson => todo!(),
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
