/// Maximum gain multiplier for main audio output (5× = +14 dB)
pub const MAX_GAIN: f32 = 5.0;
/// Minimum gain multiplier for main audio output
pub const MIN_GAIN: f32 = 0.0;

/// Maximum mic gain multiplier (3× = +9.5 dB)
pub const MAX_MIC_GAIN: f32 = 3.0;
/// Minimum mic gain multiplier (0.5× = −6 dB, prevents complete silence)
pub const MIN_MIC_GAIN: f32 = 0.5;

/// Pure clamp helpers so gain/volume bounds can be unit-tested without
/// constructing an `AudioPlayer` (which needs an audio device).
/// `f32::clamp` propagates NaN, which would feed NaN samples to rodio; we
/// substitute the lower bound so the failure mode is silence.
pub fn clamp_volume(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

pub fn clamp_gain(g: f32) -> f32 {
    if g.is_nan() {
        MIN_GAIN
    } else {
        g.clamp(MIN_GAIN, MAX_GAIN)
    }
}

pub fn clamp_mic_gain(g: f32) -> f32 {
    if g.is_nan() {
        MIN_MIC_GAIN
    } else {
        g.clamp(MIN_MIC_GAIN, MAX_MIC_GAIN)
    }
}

pub fn clamp_layer_volume(v: f32) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_volume_bounds() {
        assert_eq!(clamp_volume(-1.0), 0.0);
        assert_eq!(clamp_volume(0.0), 0.0);
        assert_eq!(clamp_volume(0.5), 0.5);
        assert_eq!(clamp_volume(1.0), 1.0);
        assert_eq!(clamp_volume(2.0), 1.0);
        assert_eq!(clamp_volume(f32::INFINITY), 1.0);
        assert_eq!(clamp_volume(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn clamp_volume_nan_pins_to_low_bound() {
        assert_eq!(clamp_volume(f32::NAN), 0.0);
    }

    #[test]
    fn clamp_gain_bounds() {
        assert_eq!(clamp_gain(MIN_GAIN - 1.0), MIN_GAIN);
        assert_eq!(clamp_gain(MIN_GAIN), MIN_GAIN);
        assert_eq!(clamp_gain(1.0), 1.0);
        assert_eq!(clamp_gain(MAX_GAIN), MAX_GAIN);
        assert_eq!(clamp_gain(MAX_GAIN + 5.0), MAX_GAIN);
        assert_eq!(clamp_gain(f32::INFINITY), MAX_GAIN);
    }

    #[test]
    fn clamp_mic_gain_bounds() {
        assert_eq!(clamp_mic_gain(0.0), MIN_MIC_GAIN);
        assert_eq!(clamp_mic_gain(MIN_MIC_GAIN - 0.1), MIN_MIC_GAIN);
        assert_eq!(clamp_mic_gain(MIN_MIC_GAIN), MIN_MIC_GAIN);
        assert_eq!(clamp_mic_gain(1.0), 1.0);
        assert_eq!(clamp_mic_gain(MAX_MIC_GAIN), MAX_MIC_GAIN);
        assert_eq!(clamp_mic_gain(10.0), MAX_MIC_GAIN);
    }

    #[test]
    fn clamp_layer_volume_bounds() {
        assert_eq!(clamp_layer_volume(-0.5), 0.0);
        assert_eq!(clamp_layer_volume(0.7), 0.7);
        assert_eq!(clamp_layer_volume(1.5), 1.0);
    }
}
