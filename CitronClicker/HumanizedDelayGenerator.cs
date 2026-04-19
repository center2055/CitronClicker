using System;

namespace CitronClicker
{
    /// <summary>
    /// Produces (UpPhaseMs, DownPhaseMs) for the butterfly-style loop: synthetic UP, wait Up, synthetic DOWN, wait Down.
    /// Down phase is the hold after LEFT DOWN until the next LEFT UP; anticheats expect ~12–25ms (timing discipline similar in spirit to public Fabric clients such as https://github.com/CCBlueX/LiquidBounce).
    /// </summary>
    public class HumanizedDelayGenerator
    {
        public const double MaxEffectiveCps = 16.5;
        public const int MinDownPhaseMs = 12;
        public const int MaxDownPhaseMs = 25;
        public const int MinUpPhaseMs = 8;

        private readonly Random random = new Random();
        private double driftTime;

        public (int UpTime, int DownTime) GetDelays(int minCps, int maxCps, bool accessibilityMode)
        {
            NormalizeAndCapRange(minCps, maxCps, out double effMin, out double effMax);
            return accessibilityMode
                ? GetDelaysAccessibility(effMin, effMax)
                : GetDelaysBypass(effMin, effMax);
        }

        private static void NormalizeAndCapRange(int minCps, int maxCps, out double effMin, out double effMax)
        {
            int lo = Math.Clamp(Math.Min(minCps, maxCps), 1, 20);
            int hi = Math.Clamp(Math.Max(minCps, maxCps), 1, 20);
            effMin = lo;
            effMax = Math.Min(hi, MaxEffectiveCps);
            if (effMin > effMax)
                effMin = effMax;
        }

        /// <summary>Stable ~1000/target CPS with ±3ms; no Gaussian drift or outliers.</summary>
        private (int UpTime, int DownTime) GetDelaysAccessibility(double effMin, double effMax)
        {
            double targetCps = (effMin + effMax) / 2.0;
            if (targetCps < 1.0)
                targetCps = 1.0;

            int basePeriod = (int)Math.Round(1000.0 / targetCps);
            basePeriod += random.Next(-3, 4);

            int downPhase = random.Next(MinDownPhaseMs, MaxDownPhaseMs + 1);
            basePeriod = Math.Max(basePeriod, downPhase + MinUpPhaseMs);

            int upTime = basePeriod - downPhase;
            return (upTime, downPhase);
        }

        /// <summary>Full variance path; enforces hard CPS cap and down-phase window for bypass / pentest profile.</summary>
        private (int UpTime, int DownTime) GetDelaysBypass(double effMin, double effMax)
        {
            double span = effMax - effMin;
            double currentCps = span <= 0
                ? effMin
                : effMin + random.NextDouble() * span;

            double baseDelay = 1000.0 / currentCps;

            double u1 = 1.0 - random.NextDouble();
            double u2 = 1.0 - random.NextDouble();
            double randStdNormal = Math.Sqrt(-2.0 * Math.Log(u1)) * Math.Sin(2.0 * Math.PI * u2);

            double stdDev = baseDelay * 0.15;
            double gaussianDelay = baseDelay + (stdDev * randStdNormal);

            driftTime += 0.1;
            double driftFactor = 1.0 + (Math.Sin(driftTime) * 0.06);
            double driftDelay = gaussianDelay * driftFactor;

            double chance = random.NextDouble();
            if (chance < 0.03)
            {
                driftDelay += random.Next(25, 76);
            }
            else if (chance < 0.05)
            {
                double maxSubtract = driftDelay * 0.40;
                double minSubtract = Math.Min(15.0, maxSubtract);
                if (maxSubtract >= 15.0)
                {
                    double subtract = random.Next((int)minSubtract, (int)maxSubtract + 1);
                    driftDelay -= subtract;
                }
                else
                {
                    driftDelay -= maxSubtract;
                }
            }

            driftDelay = Math.Max(10.0, driftDelay);
            int totalDelayMs = (int)Math.Round(driftDelay);
            totalDelayMs = Math.Max(totalDelayMs, MinDownPhaseMs + MinUpPhaseMs);

            int maxUpTimeLimit = Math.Max(11, totalDelayMs / 2);
            int upTime = random.Next(10, Math.Max(11, maxUpTimeLimit));

            int downTime = Math.Clamp(Math.Max(1, totalDelayMs - upTime), MinDownPhaseMs, MaxDownPhaseMs);
            int upTimeAdjusted = totalDelayMs - downTime;
            if (upTimeAdjusted < MinUpPhaseMs)
            {
                upTimeAdjusted = MinUpPhaseMs;
                downTime = Math.Clamp(totalDelayMs - MinUpPhaseMs, MinDownPhaseMs, MaxDownPhaseMs);
            }

            return (upTimeAdjusted, downTime);
        }
    }
}
