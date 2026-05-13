use ringbuf::{
    traits::{Consumer as ConsumerTrait, Split},
    HeapRb,
};
use rodio::Source;
use std::time::Duration;

pub(super) type MicConsumer = <HeapRb<f32> as Split>::Cons;

/// Custom rodio Source that reads captured mic audio from a lock-free SPSC ring.
///
/// The cpal input callback pushes samples; the rodio output thread pops them.
/// Returns silence on underflow rather than stopping the source — keeps the
/// mixer slot alive without clicks.
pub(super) struct MicSource {
    pub consumer: MicConsumer,
    pub sample_rate: std::num::NonZero<u32>,
    pub channels: std::num::NonZero<u16>,
}

impl Iterator for MicSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        Some(self.consumer.try_pop().unwrap_or(0.0))
    }
}

impl Source for MicSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> std::num::NonZero<u16> {
        self.channels
    }
    fn sample_rate(&self) -> std::num::NonZero<u32> {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
