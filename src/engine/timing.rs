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

/// Humanized click timing. Ported from `HumanizedDelayGenerator.GetDelays`: sqrt-biased CPS
/// sampling toward the max, Box–Muller gaussian, sine drift, rare perturbations, and 50ms
/// tick-bucket magnetization. Returns `(up_ms, down_ms)`.
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

        let u = rng.unit();
        let bias_high = if span <= 0.0 { 1.0 } else { u.sqrt() };
        let mut sample_cps = if span <= 0.0 {
            eff_min
        } else {
            eff_min + span * bias_high
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

        let rem = period % 50.0;
        if rem < 15.0 {
            period = period - rem + (rng.unit() * 4.0 - 2.0);
        } else if rem > 35.0 {
            period = period + (50.0 - rem) + (rng.unit() * 4.0 - 2.0);
        }
        period = period.max(5.0);

        // Respect the user's CPS bounds. The 50ms tick magnetization above can otherwise snap a
        // near-max period down to the 50ms (20 CPS) bucket, overshooting max. Clamp the period so
        // the resulting rate stays within [min_cps, max_cps].
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
