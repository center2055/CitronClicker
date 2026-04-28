using System;

namespace CitronClicker
{
    /// <summary>
    /// Butterfly timing: one cycle length ≈ 1000 / sampled CPS (ms). Sampling is biased toward the
    /// high end of the min–max range so average rate stays closer to max CPS (snappier for chains / combos).
    /// Variance is tight so clicks do not “float” near the low end of the slider range.
    /// </summary>
    public class HumanizedDelayGenerator
    {
        public const int MaxCpsUi = 20;

        private readonly Random random = new Random();
        private double driftTime;

        public (int UpTime, int DownTime) GetDelays(int minCps, int maxCps)
        {
            NormalizeRange(minCps, maxCps, out double effMin, out double effMax);
            double span = effMax - effMin;
            double u = random.NextDouble();
            // Bias toward upper CPS (sqrt): fewer long-period cycles when min is below max.
            double biasHigh = span <= 0 ? 1.0 : Math.Sqrt(u);
            double sampleCps = span <= 0 ? effMin : effMin + span * biasHigh;
            if (sampleCps < 1.0)
                sampleCps = 1.0;

            double targetPeriodMs = 1000.0 / sampleCps;

            double u1 = 1.0 - random.NextDouble();
            double u2 = 1.0 - random.NextDouble();
            double randStdNormal = Math.Sqrt(-2.0 * Math.Log(u1)) * Math.Sin(2.0 * Math.PI * u2);

            driftTime += 0.1;
            double driftFactor = 1.0 + Math.Sin(driftTime) * 0.04;
            double jitter = Math.Clamp(1.0 + randStdNormal * 0.05, 0.93, 1.07);
            double period = targetPeriodMs * driftFactor * jitter;

            double r = random.NextDouble();
            if (r < 0.008)
                period += random.Next(2, 10);
            else if (r < 0.02)
                period -= random.Next(1, 5);

            period = Math.Max(5.0, period);

            // Nudge full-cycle period toward 50 ms bucket boundaries (small variance), then re-clamp.
            double tickRemainder = period % 50.0;
            if (tickRemainder < 15.0)
                period = period - tickRemainder + ((random.NextDouble() * 4.0) - 2.0);
            else if (tickRemainder > 35.0)
                period = period + (50.0 - tickRemainder) + ((random.NextDouble() * 4.0) - 2.0);
            period = Math.Max(5.0, period);

            int P = (int)Math.Round(period);

            int downCap = Math.Min(26, Math.Max(3, P - 2));
            int downMin = Math.Min(3, downCap);
            int downTime = downMin >= downCap ? downCap : random.Next(downMin, downCap + 1);
            int upTime = P - downTime;
            if (upTime < 2)
            {
                downTime = Math.Clamp(P - 2, 2, downCap);
                upTime = P - downTime;
            }

            return (Math.Max(1, upTime), Math.Max(1, downTime));
        }

        private static void NormalizeRange(int minCps, int maxCps, out double effMin, out double effMax)
        {
            int lo = Math.Clamp(Math.Min(minCps, maxCps), 1, MaxCpsUi);
            int hi = Math.Clamp(Math.Max(minCps, maxCps), 1, MaxCpsUi);
            effMin = lo;
            effMax = hi;
            if (effMin > effMax)
                effMin = effMax;
        }
    }
}
