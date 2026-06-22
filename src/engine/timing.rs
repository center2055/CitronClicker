//! OS-agnostic timing math: a small xoshiro256** RNG, the humanized delay generator ported
//! from the old Citron `HumanizedDelayGenerator`, the fixed (humanize-off) period, and the
//! smooth jitter generator. All delays are in milliseconds; `get_delays` returns (up, down).

use std::f64::consts::{PI, TAU};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CPS_UI: f64 = 20.0;

/// xoshiro256** seeded via SplitMix64 from the system clock + a per-side salt.
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn seeded(salt: u64) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x1234_5678_9ABC_DEF0);
        let mut x = nanos ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
        let mut sm = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let s = [sm(), sm(), sm(), sm()];
        // Avoid the all-zero state.
        if s == [0; 4] {
            return Self { s: [1, 2, 3, 4] };
        }
        Self { s }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform f64 in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform integer in [lo, hi).
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            lo
        } else {
            lo + (self.next_u64() % ((hi - lo) as u64)) as i32
        }
    }
}

/// Humanized click timing (evolved from `HumanizedDelayGenerator.GetDelays`). Returns
/// `(up_ms, down_ms)`. CPS is sampled with density proportional to the rate so the realized
/// wall-clock average equals the [min,max] midpoint (see `get_delays`), then perturbed by a
/// Box–Muller gaussian and a slow sine drift, nudged by rare ms-scale jitters, and finally clamped
/// so the period stays within the [min,max] bounds.
pub struct HumanizedDelay {
    drift: f64,
}

impl HumanizedDelay {
    pub fn new() -> Self {
        Self { drift: 0.0 }
    }

    pub fn get_delays(&mut self, min_cps: f32, max_cps: f32, rng: &mut Rng) -> (f64, f64) {
        let lo = (min_cps.min(max_cps) as f64).clamp(1.0, MAX_CPS_UI);
        let hi = (min_cps.max(max_cps) as f64).clamp(1.0, MAX_CPS_UI);
        let eff_min = lo;
        let eff_max = hi.max(lo);
        let span = eff_max - eff_min;

        // Sample CPS with density proportional to the rate (inverse-CDF of a linear pdf:
        // sqrt(min^2 + u*(max^2 - min^2))). Sampling CPS *uniformly* feels right but is wrong for
        // the wall-clock rate a CPS test measures: a slow click occupies far more elapsed time than
        // a fast one (a 1-CPS click is a full second, a 20-CPS click is 50ms), so uniform sampling
        // lets the slow end dominate the timeline and the measured rate collapses to the logarithmic
        // mean (1..20 -> ~6.3, not 10.5). Weighting by rate cancels that 1/cps time-dilation exactly,
        // so the long-run measured CPS equals the [min,max] midpoint at any range width.
        let u = rng.unit();
        let mut sample_cps = if span <= 0.0 {
            eff_min
        } else {
            (eff_min * eff_min + u * (eff_max * eff_max - eff_min * eff_min)).sqrt()
        };
        if sample_cps < 1.0 {
            sample_cps = 1.0;
        }
        let target_period = 1000.0 / sample_cps;

        let u1 = 1.0 - rng.unit();
        let u2 = 1.0 - rng.unit();
        let std_normal = (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).sin();

        self.drift += 0.1;
        if self.drift > TAU {
            self.drift -= TAU; // keep sin() precise over long sessions
        }
        let drift_factor = 1.0 + self.drift.sin() * 0.04;
        let jitter = (1.0 + std_normal * 0.05).clamp(0.93, 1.07);
        let mut period = target_period * drift_factor * jitter;

        let r = rng.unit();
        if r < 0.008 {
            period += rng.range(2, 10) as f64;
        } else if r < 0.02 {
            period -= rng.range(1, 5) as f64;
        }
        period = period.max(5.0);

        // Keep the resulting rate within the user's [min_cps, max_cps] bounds. (The old 50ms
        // tick-magnetization was removed — it snapped periods toward the 50/60ms buckets, which
        // skewed the average away from the midpoint and could overshoot max.)
        let min_period = 1000.0 / eff_max; // fastest allowed
        let max_period = 1000.0 / eff_min; // slowest allowed
        period = period.clamp(min_period, max_period);

        let p = period.round() as i32;
        let down_cap = 26.min(3.max(p - 2));
        let down_min = 3.min(down_cap);
        let mut down = if down_min >= down_cap {
            down_cap
        } else {
            rng.range(down_min, down_cap + 1)
        };
        let mut up = p - down;
        if up < 2 {
            down = (p - 2).clamp(2, down_cap);
            up = p - down;
        }
        (up.max(1) as f64, down.max(1) as f64)
    }
}

/// Fixed (humanize-off) timing: a perfectly periodic interval — more effective, more
/// detectable. Returns `(up_ms, down_ms)`.
pub fn fixed_delays(cps: f32) -> (f64, f64) {
    let cps = (cps as f64).clamp(1.0, MAX_CPS_UI);
    let total = 1000.0 / cps;
    let down = (total * 0.25).clamp(3.0, 25.0);
    let up = (total - down).max(1.0);
    (up, down)
}

/// Smooth jitter (ported from `SmoothJitterLoop`): sine/cos offsets scaled by intensity.
pub struct SmoothJitter {
    time: f64,
}

impl SmoothJitter {
    pub fn new() -> Self {
        Self { time: 0.0 }
    }
    pub fn reset(&mut self) {
        self.time = 0.0;
    }
    pub fn next(&mut self, intensity: i32, rng: &mut Rng) -> Option<(i32, i32)> {
        self.time += 0.35;
        if self.time > TAU {
            self.time -= TAU;
        }
        let i = intensity as f64;
        let jx = self.time.sin() * i;
        let jy = (self.time * 0.8).cos() * (i * 0.4);
        let ix = (jx * (rng.unit() * 0.6 + 0.4)).round() as i32;
        let iy = (jy * (rng.unit() * 0.6 + 0.4)).round() as i32;
        if ix != 0 || iy != 0 {
            Some((ix, iy))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Realized wall-clock CPS over a long run = total clicks / total elapsed time.
    fn measured_cps(min: f32, max: f32, clicks: u32) -> f64 {
        let mut rng = Rng::seeded(0xC174_0);
        let mut hd = HumanizedDelay::new();
        let mut total_ms = 0.0;
        for _ in 0..clicks {
            let (up, down) = hd.get_delays(min, max, &mut rng);
            total_ms += up + down; // one full click cycle
        }
        clicks as f64 * 1000.0 / total_ms
    }

    #[test]
    fn measured_rate_tracks_midpoint() {
        // The number a CPS test reports must land on the slider midpoint, regardless of how wide
        // the range is — the wide 1..20 case is the one that used to read ~6.3 instead of 10.5.
        for (min, max) in [(1.0f32, 20.0f32), (5.0, 15.0), (10.0, 12.0), (15.0, 20.0)] {
            let mid = (min + max) as f64 / 2.0;
            let got = measured_cps(min, max, 400_000);
            let err = (got - mid).abs() / mid;
            assert!(
                err < 0.04,
                "range {min}-{max}: expected ~{mid} cps, measured {got:.2} ({:.1}% off)",
                err * 100.0
            );
        }
    }
}
