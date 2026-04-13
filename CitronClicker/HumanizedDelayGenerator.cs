using System;

namespace CitronClicker
{
    public class HumanizedDelayGenerator
    {
        private Random random = new Random();
        private double driftTime = 0;

        public (int UpTime, int DownTime) GetDelays(int minCps, int maxCps, bool comboMode)
        {
            double currentCps;
            if (comboMode)
            {
                double bias = minCps + ((maxCps - minCps) * 0.8);
                double swing = Math.Max(0.5, (maxCps - minCps) * 0.6);
                currentCps = bias + ((random.NextDouble() * 2.0 - 1.0) * swing);
                currentCps = Math.Clamp(currentCps, minCps, maxCps);
            }
            else
            {
                currentCps = minCps + (random.NextDouble() * (maxCps - minCps));
            }

            double baseDelay = 1000.0 / currentCps;

            // Box-Muller Transform
            double u1 = 1.0 - random.NextDouble(); 
            double u2 = 1.0 - random.NextDouble();
            double randStdNormal = Math.Sqrt(-2.0 * Math.Log(u1)) * Math.Sin(2.0 * Math.PI * u2);

            double stdDev = baseDelay * 0.15; 
            double gaussianDelay = baseDelay + (stdDev * randStdNormal);

            // Sine-Wave Drift
            driftTime += 0.1;
            double driftFactor = 1.0 + (Math.Sin(driftTime) * 0.06);
            double driftDelay = gaussianDelay * driftFactor;

            // Outlier Injection
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

            // Clamp
            driftDelay = Math.Max(10.0, driftDelay);
            int totalDelayMs = (int)Math.Round(driftDelay);

            int upTime;
            if (comboMode)
            {
                double ratio = 0.20 + (random.NextDouble() * 0.20);
                upTime = Math.Clamp((int)Math.Round(totalDelayMs * ratio), 10, Math.Max(12, totalDelayMs / 2));
            }
            else
            {
                int maxUpTimeLimit = Math.Max(11, totalDelayMs / 2);
                upTime = random.Next(10, Math.Max(11, maxUpTimeLimit));
            }

            int downTime = Math.Max(1, totalDelayMs - upTime);

            return (upTime, downTime);
        }
    }
}
