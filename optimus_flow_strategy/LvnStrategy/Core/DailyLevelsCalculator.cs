using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Calculates daily levels (PDH/PDL, VAH/VAL, POC, ONH/ONL) from historical bar data.
/// Matches Rust implementation from src/trading_core/daily_levels.rs
/// </summary>
public static class DailyLevelsCalculator
{
    /// <summary>
    /// Bucket size for volume profile calculation ($1.0 per bucket)
    /// </summary>
    private const double BucketSize = 1.0;

    /// <summary>
    /// Value Area percentage (70% of volume)
    /// </summary>
    private const double ValueAreaPercent = 0.70;

    /// <summary>
    /// Calculate daily levels from RTH and overnight session bars
    /// </summary>
    /// <param name="rthBars">Regular trading hours bars (9:30 AM - 4:00 PM ET)</param>
    /// <param name="overnightBars">Overnight session bars (6:00 PM - 9:30 AM ET)</param>
    public static DailyLevels Calculate(IEnumerable<Bar> rthBars, IEnumerable<Bar>? overnightBars = null)
    {
        var rthList = rthBars.ToList();
        var onList = overnightBars?.ToList() ?? new List<Bar>();

        if (rthList.Count == 0)
        {
            throw new ArgumentException("RTH bars cannot be empty");
        }

        // Calculate PDH/PDL from RTH session
        var pdh = rthList.Max(b => b.High);
        var pdl = rthList.Min(b => b.Low);

        // Calculate ONH/ONL from overnight session
        var onh = onList.Count > 0 ? onList.Max(b => b.High) : pdh;
        var onl = onList.Count > 0 ? onList.Min(b => b.Low) : pdl;

        // Calculate volume profile and value area from RTH
        var (poc, vah, val) = CalculateValueArea(rthList);

        return new DailyLevels
        {
            Date = DateOnly.FromDateTime(rthList[0].Timestamp),
            Pdh = pdh,
            Pdl = pdl,
            Onh = onh,
            Onl = onl,
            Poc = poc,
            Vah = vah,
            Val = val,
            SessionHigh = pdh,
            SessionLow = pdl
        };
    }

    /// <summary>
    /// Calculate POC and Value Area from bars using TPO-style volume profile
    /// </summary>
    private static (double Poc, double Vah, double Val) CalculateValueArea(List<Bar> bars)
    {
        if (bars.Count == 0)
            return (0, 0, 0);

        // Build volume profile with fixed bucket size
        var volumeProfile = new Dictionary<double, ulong>();

        foreach (var bar in bars)
        {
            // Distribute volume across the bar's range in buckets
            var lowBucket = Math.Floor(bar.Low / BucketSize) * BucketSize;
            var highBucket = Math.Floor(bar.High / BucketSize) * BucketSize;

            // For simplicity, assign all volume to the close price bucket
            // (More sophisticated: distribute proportionally across range)
            var closeBucket = Math.Floor(bar.Close / BucketSize) * BucketSize;

            if (!volumeProfile.ContainsKey(closeBucket))
                volumeProfile[closeBucket] = 0;

            volumeProfile[closeBucket] += bar.Volume;
        }

        if (volumeProfile.Count == 0)
            return (0, 0, 0);

        // Find POC (Point of Control) = bucket with maximum volume
        var poc = volumeProfile.MaxBy(kv => kv.Value).Key;

        // Calculate total volume
        var totalVolume = volumeProfile.Values.Aggregate(0UL, (a, b) => a + b);
        var targetVolume = (ulong)(totalVolume * ValueAreaPercent);

        // Expand from POC to find Value Area (70% of volume)
        var vahBucket = poc;
        var valBucket = poc;
        var accumulatedVolume = volumeProfile[poc];

        var sortedBuckets = volumeProfile.Keys.OrderBy(k => k).ToList();
        var pocIndex = sortedBuckets.IndexOf(poc);
        var upperIndex = pocIndex + 1;
        var lowerIndex = pocIndex - 1;

        while (accumulatedVolume < targetVolume && (upperIndex < sortedBuckets.Count || lowerIndex >= 0))
        {
            var upperVol = upperIndex < sortedBuckets.Count
                ? volumeProfile[sortedBuckets[upperIndex]]
                : 0UL;
            var lowerVol = lowerIndex >= 0
                ? volumeProfile[sortedBuckets[lowerIndex]]
                : 0UL;

            if (upperVol >= lowerVol && upperIndex < sortedBuckets.Count)
            {
                accumulatedVolume += upperVol;
                vahBucket = sortedBuckets[upperIndex];
                upperIndex++;
            }
            else if (lowerIndex >= 0)
            {
                accumulatedVolume += lowerVol;
                valBucket = sortedBuckets[lowerIndex];
                lowerIndex--;
            }
            else if (upperIndex < sortedBuckets.Count)
            {
                accumulatedVolume += upperVol;
                vahBucket = sortedBuckets[upperIndex];
                upperIndex++;
            }
            else
            {
                break;
            }
        }

        return (poc + BucketSize / 2, vahBucket + BucketSize, valBucket);
    }

    /// <summary>
    /// Check if a timestamp is within RTH hours (9:30 AM - 4:00 PM Eastern)
    /// </summary>
    public static bool IsRthHour(DateTime utcTime)
    {
        // Convert to Eastern Time
        var eastern = TimeZoneInfo.FindSystemTimeZoneById("Eastern Standard Time");
        var etTime = TimeZoneInfo.ConvertTimeFromUtc(utcTime, eastern);

        var hour = etTime.Hour;
        var minute = etTime.Minute;

        // RTH is 9:30 AM to 4:00 PM ET
        if (hour < 9 || hour >= 16) return false;
        if (hour == 9 && minute < 30) return false;

        return true;
    }

    /// <summary>
    /// Check if a timestamp is within overnight session (6:00 PM - 9:30 AM Eastern)
    /// </summary>
    public static bool IsOvernightHour(DateTime utcTime)
    {
        var eastern = TimeZoneInfo.FindSystemTimeZoneById("Eastern Standard Time");
        var etTime = TimeZoneInfo.ConvertTimeFromUtc(utcTime, eastern);

        var hour = etTime.Hour;
        var minute = etTime.Minute;

        // Overnight is 6:00 PM to 9:30 AM ET
        if (hour >= 18) return true;  // 6 PM to midnight
        if (hour < 9) return true;     // midnight to 9 AM
        if (hour == 9 && minute < 30) return true; // 9:00 to 9:30 AM

        return false;
    }
}
