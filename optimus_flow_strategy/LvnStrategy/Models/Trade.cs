namespace LvnStrategy.Models;

/// <summary>
/// Trade side enumeration.
/// Matches Rust enum from src/trading_core/trades.rs
/// </summary>
public enum Side
{
    Buy,
    Sell
}

/// <summary>
/// Raw trade from market data.
/// Matches Rust struct from src/trading_core/trades.rs
/// </summary>
public class Trade
{
    public DateTime TsEvent { get; set; }
    public double Price { get; set; }
    public ulong Size { get; set; }
    public Side Side { get; set; }
    public string Symbol { get; set; } = "";
}
