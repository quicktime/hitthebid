namespace LvnStrategy.Models;

/// <summary>
/// Trade direction
/// </summary>
public enum Direction
{
    Long,
    Short
}

/// <summary>
/// Active position state for tracking open trades.
/// Matches Rust struct from src/trading_core/trader.rs
/// </summary>
public class OpenPosition
{
    /// <summary>Long or Short</summary>
    public Direction Direction { get; set; }

    /// <summary>Price at which position was entered</summary>
    public double EntryPrice { get; set; }

    /// <summary>Time of entry</summary>
    public DateTime EntryTime { get; set; }

    /// <summary>LVN level that triggered the trade</summary>
    public double LevelPrice { get; set; }

    /// <summary>Initial stop loss price</summary>
    public double InitialStop { get; set; }

    /// <summary>Take profit target price (0 = trailing stop only)</summary>
    public double TakeProfit { get; set; }

    /// <summary>Current trailing stop price</summary>
    public double TrailingStop { get; set; }

    /// <summary>Highest price reached since entry (for trailing stop)</summary>
    public double HighestPrice { get; set; }

    /// <summary>Lowest price reached since entry (for trailing stop)</summary>
    public double LowestPrice { get; set; }

    /// <summary>Number of bars since entry</summary>
    public int BarCount { get; set; }

    /// <summary>Number of contracts</summary>
    public int Contracts { get; set; } = 1;

    /// <summary>
    /// Calculate unrealized P&L in points
    /// </summary>
    public double UnrealizedPnl(double currentPrice)
    {
        return Direction == Direction.Long
            ? currentPrice - EntryPrice
            : EntryPrice - currentPrice;
    }

    /// <summary>
    /// Check if trailing stop should be updated based on new price
    /// </summary>
    public bool ShouldUpdateTrailingStop(double currentPrice, double trailingDistance)
    {
        if (Direction == Direction.Long)
        {
            if (currentPrice > HighestPrice)
            {
                var newStop = currentPrice - trailingDistance;
                return newStop > TrailingStop;
            }
        }
        else
        {
            if (currentPrice < LowestPrice)
            {
                var newStop = currentPrice + trailingDistance;
                return newStop < TrailingStop;
            }
        }
        return false;
    }

    /// <summary>
    /// Check if stop loss has been hit
    /// </summary>
    public bool IsStopHit(double currentPrice)
    {
        return Direction == Direction.Long
            ? currentPrice <= TrailingStop
            : currentPrice >= TrailingStop;
    }

    /// <summary>
    /// Check if take profit has been hit
    /// </summary>
    public bool IsTakeProfitHit(double currentPrice)
    {
        if (TakeProfit == 0) return false;
        return Direction == Direction.Long
            ? currentPrice >= TakeProfit
            : currentPrice <= TakeProfit;
    }
}
