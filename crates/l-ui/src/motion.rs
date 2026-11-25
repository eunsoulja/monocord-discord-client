//! Small animation helpers: easing curves, durations, intensity scaling.

/// Easing curve applied to a 0..1 progress value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    /// Linear.
    Linear,
    /// Ease out cubic — default for UI reveals.
    OutCubic,
    /// Ease in-out cubic.
    InOutCubic,
    /// Ease out quad — snappier.
    OutQuad,
}

impl Ease {
    /// Apply the curve to `t` in `0.0..=1.0`.
    #[must_use]
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::OutCubic => 1.0 - (1.0 - t).powi(3),
            Self::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::OutQuad => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

/// Animation intensity as understood by the UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Intensity {
    /// Instant — no animation.
    Off,
    /// Subtle, fast.
    #[default]
    Subtle,
    /// Full.
    Full,
}

impl Intensity {
    /// Scale a nominal duration (ms) by intensity.
    #[must_use]
    pub fn scale(self, ms: f32) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Subtle => ms * 0.6,
            Self::Full => ms,
        }
    }

    /// Whether animations are enabled at all.
    #[must_use]
    pub fn enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Standard durations in milliseconds (pre-scaling).
pub mod dur {
    /// Fast hover/press feedback.
    pub const FAST: f32 = 80.0;
    /// Panel/sidebar transitions.
    pub const MEDIUM: f32 = 160.0;
    /// Page-level transitions (used sparingly).
    pub const SLOW: f32 = 240.0;
}

/// Progress of an animation started `elapsed` ms ago with `duration` ms.
/// Returns eased 0..1, or `None` if complete.
#[must_use]
pub fn progress(elapsed_ms: f32, duration_ms: f32, ease: Ease) -> Option<f32> {
    if duration_ms <= 0.0 {
        return None;
    }
    let t = elapsed_ms / duration_ms;
    if t >= 1.0 { None } else { Some(ease.apply(t)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_endpoints() {
        for e in [
            Ease::Linear,
            Ease::OutCubic,
            Ease::InOutCubic,
            Ease::OutQuad,
        ] {
            assert!((e.apply(0.0) - 0.0).abs() < 1e-6);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn intensity_scales() {
        assert_eq!(Intensity::Off.scale(100.0), 0.0);
        assert!((Intensity::Subtle.scale(100.0) - 60.0).abs() < 1e-3);
        assert!((Intensity::Full.scale(100.0) - 100.0).abs() < 1e-3);
    }
}
