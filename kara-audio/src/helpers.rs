use dasp::{
    interpolate::sinc::Sinc,
    ring_buffer,
    signal::{self, interpolate::Converter},
    Sample, Signal,
};

use crate::StreamDevice;

pub fn stereo_to_mono(input_data: &[i16]) -> Vec<i16> {
    let mut result = Vec::with_capacity(input_data.len() / 2);
    result.extend(
        input_data
            .chunks_exact(2)
            .map(|chunk| chunk[0] / 2 + chunk[1] / 2),
    );
    result
}

pub(crate) fn set_sample_rate(data: &[f32], stream_device: &StreamDevice) -> Vec<i16> {
    // resample samples
    if stream_device.sample_rate != 16000 {
        let samples: Vec<_> = data.iter().map(|f| f.to_sample::<f64>()).collect();
        let signal = signal::from_interleaved_samples_iter::<_, _>(samples.iter().cloned());
        // Convert the signal's sample rate using `Sinc` interpolation.
        let ring_buffer = ring_buffer::Fixed::from([[0.0]; 100]);
        let sinc = Sinc::new(ring_buffer);

        let new_signal =
            Converter::from_hz_to_hz(signal, sinc, stream_device.sample_rate.into(), 16000.0_f64);
        new_signal
            .until_exhausted()
            .map(|f| f[0].to_sample::<i16>())
            .collect()
    } else {
        data.iter().map(|f| f.to_sample()).collect()
    }
}
