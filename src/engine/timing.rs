//! timing math: xoshiro256** rng, the humanized delay generator, the fixed (humanize-off)
//! period, and smooth jitter. all delays in ms; get_delays returns (up, down).

use std::f64::consts::{PI, TAU};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CPS_UI: f64 = 20.0;

/// xoshiro256**, seeded from the clock + a per-side salt via splitmix64
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
        // all-zero state is degenerate
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

    /// f64 in [0,1)
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// int in [lo,hi)
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            lo
        } else {
            lo + (self.next_u64() % ((hi - lo) as u64)) as i32
        }
    }
}

/// humanized click timing. returns (up_ms, down_ms). cps is sampled weighted by rate so the
/// measured average lands on the [min,max] midpoint, then nudged by a gaussian + slow sine drift.
/// core period is clamped to the range, then a few clicks get a hesitation/flick past the edges
/// for natural tails instead of hard walls.
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

        // sample cps weighted by rate (inverse-cdf of a linear pdf). sampling uniformly looks right
        // but tanks the measured rate: a slow click eats far more wall-clock than a fast one, so the
        // slow end dominates and 1..20 reads ~6.3 not 10.5. weighting cancels the 1/cps dilation so
        // the average = midpoint at any width.
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
            self.drift -= TAU; // keep sin() sane over long runs
        }
        let drift_factor = 1.0 + self.drift.sin() * 0.04;
        let jitter = (1.0 + std_normal * 0.05).clamp(0.93, 1.07);
        let mut period = target_period * drift_factor * jitter;

        // clamp the core period to the range so the bulk stays in bounds and the average holds.
        // (old 50ms tick-snapping is gone — it skewed the average and could overshoot max.)
        let min_period = 1000.0 / eff_max; // fastest
        let max_period = 1000.0 / eff_min; // slowest
        period = period.clamp(min_period, max_period);

        // tails, after the clamp so they survive (clamping first = hard wall, no tail, botlike).
        // a few clicks drift past the edges: a brief hesitation (slow) or a quick flick (fast).
        let r = rng.unit();
        if r < 0.006 {
            period += rng.range(8, 30) as f64; // hesitation, slow tail
        } else if r < 0.018 {
            period += rng.range(2, 8) as f64; // small drift slower
        } else if r < 0.030 {
            period -= rng.range(2, 8) as f64; // flick, fast tail
        }
        period = period.max(5.0);

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

/// fixed (humanize-off) timing — perfectly periodic. more effective, easier to flag.
pub fn fixed_delays(cps: f32) -> (f64, f64) {
    let cps = (cps as f64).clamp(1.0, MAX_CPS_UI);
    let total = 1000.0 / cps;
    let down = (total * 0.25).clamp(3.0, 25.0);
    let up = (total - down).max(1.0);
    (up, down)
}

/// smooth jitter: sine/cos offsets scaled by intensity
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

    /// measured cps over a long run = clicks / elapsed time
    fn measured_cps(min: f32, max: f32, clicks: u32) -> f64 {
        let mut rng = Rng::seeded(0xC174_0);
        let mut hd = HumanizedDelay::new();
        let mut total_ms = 0.0;
        for _ in 0..clicks {
            let (up, down) = hd.get_delays(min, max, &mut rng);
            total_ms += up + down; // full click cycle
        }
        clicks as f64 * 1000.0 / total_ms
    }

    #[test]
    fn measured_rate_tracks_midpoint() {
        // a cps test must read the slider midpoint at any width — 1..20 used to read ~6.3 not 10.5
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

    #[test]
    fn distribution_has_tails_but_stays_mostly_in_range() {
        // clamp keeps the bulk in [min,max]; the post-clamp hesitation/flick spills a few past the
        // edges for soft tails, but the vast majority stay in range
        let (min, max) = (12.0f32, 18.0f32);
        let min_p = 1000.0 / max as f64; // fastest
        let max_p = 1000.0 / min as f64; // slowest
        let mut rng = Rng::seeded(0x7A11_5);
        let mut hd = HumanizedDelay::new();
        let n = 200_000u32;
        let (mut slow_tail, mut fast_tail) = (0u32, 0u32);
        for _ in 0..n {
            let (up, down) = hd.get_delays(min, max, &mut rng);
            let p = up + down;
            if p > max_p + 0.5 {
                slow_tail += 1;
            } else if p < min_p - 0.5 {
                fast_tail += 1;
            }
        }
        let out = slow_tail + fast_tail;
        let frac = out as f64 / n as f64;
        // tails both ways...
        assert!(slow_tail > 0 && fast_tail > 0, "expected tails both ways: slow={slow_tail} fast={fast_tail}");
        // ...but a small minority
        assert!(frac > 0.005 && frac < 0.06, "out-of-range fraction {:.3} should be a small minority", frac);
    }
}
