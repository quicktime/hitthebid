using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Extracts Low Volume Nodes (LVNs) from trade data by building volume profiles.
/// LVNs are price levels with minimal volume that often act as support/resistance.
/// Matches Rust implementation from src/trading_core/lvn.rs
/// </summary>
public static class LvnExtractor
{
    /// <summary>
    /// Default bucket size for volume profile (0.5 points)
    /// </summary>
    public const double DefaultBucketSize = 0.5;

    /// <summary>
    /// Default threshold ratio for LVN qualification (15% of average)
    /// </summary>
    public const double DefaultThresholdRatio = 0.15;

    /// <summary>
    /// Extract LVNs from a list of trades
    /// </summary>
    /// <param name="trades">Trades from the impulse leg</param>
    /// <param name="direction">Direction of the impulse</param>
    /// <param name="impulseId">Unique ID of the impulse</param>
    /// <param name="bucketSize">Size of each price bucket in points</param>
    /// <param name="thresholdRatio">Volume ratio threshold (levels below this qualify as LVN)</param>
    public static List<LvnLevel> ExtractFromTrades(
        IEnumerable<Trade> trades,
        ImpulseDirection direction,
        Guid impulseId,
        double bucketSize = DefaultBucketSize,
        double thresholdRatio = DefaultThresholdRatio)
    {
        var tradeList = trades.ToList();
        if (tradeList.Count == 0) return new List<LvnLevel>();

        // Build volume profile
        var volumeProfile = new Dictionary<double, ulong>();

        foreach (var trade in tradeList)
        {
            var bucket = Math.Floor(trade.Price / bucketSize) * bucketSize;

            if (!volumeProfile.ContainsKey(bucket))
                volumeProfile[bucket] = 0;

            volumeProfile[bucket] += trade.Size;
        }

        if (volumeProfile.Count == 0) return new List<LvnLevel>();

        // Calculate average volume per bucket
        var totalVolume = volumeProfile.Values.Aggregate(0UL, (a, b) => a + b);
        var avgVolume = (double)totalVolume / volumeProfile.Count;

        // Find LVNs (buckets with volume below threshold)
        var lvns = new List<LvnLevel>();
        var startTime = tradeList.First().TsEvent;
        var endTime = tradeList.Last().TsEvent;
        var date = DateOnly.FromDateTime(startTime);
        var symbol = tradeList.First().Symbol;

        foreach (var (price, volume) in volumeProfile)
        {
            var volumeRatio = avgVolume > 0 ? volume / avgVolume : 1.0;

            if (volumeRatio < thresholdRatio)
            {
                lvns.Add(new LvnLevel
                {
                    ImpulseId = impulseId,
                    Price = price + bucketSize / 2, // Center of bucket
                    Volume = volume,
                    AvgVolume = avgVolume,
                    VolumeRatio = volumeRatio,
                    ImpulseStartTime = startTime,
                    ImpulseEndTime = endTime,
                    ImpulseDirection = direction,
                    Date = date,
                    Symbol = symbol
                });
            }
        }

        // Sort by price (ascending for longs, descending for shorts)
        lvns = direction == ImpulseDirection.Up
            ? lvns.OrderBy(l => l.Price).ToList()
            : lvns.OrderByDescending(l => l.Price).ToList();

        return lvns;
    }

    /// <summary>
    /// Extract LVNs from bars (using bar volume distribution)
    /// </summary>
    public static List<LvnLevel> ExtractFromBars(
        IEnumerable<Bar> bars,
        ImpulseDirection direction,
        Guid impulseId,
        double bucketSize = DefaultBucketSize,
        double thresholdRatio = DefaultThresholdRatio)
    {
        var barList = bars.ToList();
        if (barList.Count == 0) return new List<LvnLevel>();

        // Build volume profile
        var volumeProfile = new Dictionary<double, ulong>();

        foreach (var bar in barList)
        {
            // Assign all volume to the close price bucket
            // (Could be more sophisticated with VWAP distribution)
            var bucket = Math.Floor(bar.Close / bucketSize) * bucketSize;

            if (!volumeProfile.ContainsKey(bucket))
                volumeProfile[bucket] = 0;

            volumeProfile[bucket] += bar.Volume;
        }

        if (volumeProfile.Count == 0) return new List<LvnLevel>();

        // Calculate average volume
        var totalVolume = volumeProfile.Values.Aggregate(0UL, (a, b) => a + b);
        var avgVolume = (double)totalVolume / volumeProfile.Count;

        // Find LVNs
        var lvns = new List<LvnLevel>();
        var startTime = barList.First().Timestamp;
        var endTime = barList.Last().Timestamp;
        var date = DateOnly.FromDateTime(startTime);
        var symbol = barList.First().Symbol;

        foreach (var (price, volume) in volumeProfile)
        {
            var volumeRatio = avgVolume > 0 ? volume / avgVolume : 1.0;

            if (volumeRatio < thresholdRatio)
            {
                lvns.Add(new LvnLevel
                {
                    ImpulseId = impulseId,
                    Price = price + bucketSize / 2,
                    Volume = volume,
                    AvgVolume = avgVolume,
                    VolumeRatio = volumeRatio,
                    ImpulseStartTime = startTime,
                    ImpulseEndTime = endTime,
                    ImpulseDirection = direction,
                    Date = date,
                    Symbol = symbol
                });
            }
        }

        // Sort by price
        lvns = direction == ImpulseDirection.Up
            ? lvns.OrderBy(l => l.Price).ToList()
            : lvns.OrderByDescending(l => l.Price).ToList();

        return lvns;
    }

    /// <summary>
    /// Filter LVNs to only include those within a price range
    /// </summary>
    public static List<LvnLevel> FilterByRange(
        IEnumerable<LvnLevel> lvns,
        double minPrice,
        double maxPrice)
    {
        return lvns.Where(l => l.Price >= minPrice && l.Price <= maxPrice).ToList();
    }

    /// <summary>
    /// Find the nearest LVN to a given price
    /// </summary>
    public static LvnLevel? FindNearest(IEnumerable<LvnLevel> lvns, double price)
    {
        return lvns.MinBy(l => Math.Abs(l.Price - price));
    }

    /// <summary>
    /// Check if price is at an LVN level (within tolerance)
    /// </summary>
    public static LvnLevel? GetLvnAtPrice(
        IEnumerable<LvnLevel> lvns,
        double price,
        double tolerance = 2.0)
    {
        return lvns.FirstOrDefault(l => Math.Abs(l.Price - price) <= tolerance);
    }
}
