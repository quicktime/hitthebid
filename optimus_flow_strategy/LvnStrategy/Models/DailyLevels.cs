namespace LvnStrategy.Models;

/// <summary>
/// Daily trading levels including prior day high/low, overnight, and value area.
/// Matches Rust struct LiveDailyLevels from src/trading_core/state_machine.rs
/// </summary>
public class DailyLevels
{
    /// <summary>Date these levels are for</summary>
    public DateOnly Date { get; set; }

    /// <summary>Prior Day High</summary>
    public double Pdh { get; set; }

    /// <summary>Prior Day Low</summary>
    public double Pdl { get; set; }

    /// <summary>Overnight High</summary>
    public double Onh { get; set; }

    /// <summary>Overnight Low</summary>
    public double Onl { get; set; }

    /// <summary>Point of Control (highest volume price)</summary>
    public double Poc { get; set; }

    /// <summary>Value Area High (upper 70% volume boundary)</summary>
    public double Vah { get; set; }

    /// <summary>Value Area Low (lower 70% volume boundary)</summary>
    public double Val { get; set; }

    /// <summary>Current session high</summary>
    public double SessionHigh { get; set; }

    /// <summary>Current session low</summary>
    public double SessionLow { get; set; }

    /// <summary>
    /// Check if price is near a key level (within tolerance points)
    /// </summary>
    public bool IsNearLevel(double price, double tolerance = 2.0)
    {
        return Math.Abs(price - Pdh) <= tolerance ||
               Math.Abs(price - Pdl) <= tolerance ||
               Math.Abs(price - Vah) <= tolerance ||
               Math.Abs(price - Val) <= tolerance ||
               Math.Abs(price - Onh) <= tolerance ||
               Math.Abs(price - Onl) <= tolerance;
    }

    /// <summary>
    /// Get the nearest key level to the given price
    /// </summary>
    public (string Name, double Price) GetNearestLevel(double price)
    {
        var levels = new (string Name, double Price)[]
        {
            ("PDH", Pdh),
            ("PDL", Pdl),
            ("VAH", Vah),
            ("VAL", Val),
            ("ONH", Onh),
            ("ONL", Onl)
        };

        return levels.MinBy(l => Math.Abs(l.Price - price));
    }
}
