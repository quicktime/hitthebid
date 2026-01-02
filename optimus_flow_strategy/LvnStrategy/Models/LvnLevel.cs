namespace LvnStrategy.Models;

/// <summary>
/// Direction of the impulse that created this LVN
/// </summary>
public enum ImpulseDirection
{
    Up,
    Down
}

/// <summary>
/// Low Volume Node - a price level with minimal volume in the impulse profile.
/// These levels often act as support/resistance on retest.
/// Matches Rust struct from src/trading_core/lvn.rs
/// </summary>
public class LvnLevel
{
    /// <summary>Unique ID of the impulse that created this LVN</summary>
    public Guid ImpulseId { get; set; }

    /// <summary>Price level of the LVN</summary>
    public double Price { get; set; }

    /// <summary>Volume at this price level</summary>
    public ulong Volume { get; set; }

    /// <summary>Average volume across all price levels in the impulse</summary>
    public double AvgVolume { get; set; }

    /// <summary>Volume ratio (Actual/Average) - values below 0.15 qualify as LVN</summary>
    public double VolumeRatio { get; set; }

    /// <summary>When the impulse started</summary>
    public DateTime ImpulseStartTime { get; set; }

    /// <summary>When the impulse ended</summary>
    public DateTime ImpulseEndTime { get; set; }

    /// <summary>Direction of the impulse that created this LVN</summary>
    public ImpulseDirection ImpulseDirection { get; set; }

    /// <summary>Date of the LVN</summary>
    public DateOnly Date { get; set; }

    /// <summary>Symbol this LVN is for</summary>
    public string Symbol { get; set; } = "";

    /// <summary>
    /// Check if this LVN qualifies based on volume threshold
    /// </summary>
    public bool IsValid(double thresholdRatio = 0.15)
    {
        return VolumeRatio < thresholdRatio;
    }
}
