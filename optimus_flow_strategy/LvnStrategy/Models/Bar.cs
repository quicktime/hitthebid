namespace LvnStrategy.Models;

/// <summary>
/// OHLCV bar with delta (buy volume - sell volume).
/// Matches Rust struct from src/trading_core/bars.rs
/// </summary>
public class Bar
{
    public DateTime Timestamp { get; set; }
    public double Open { get; set; }
    public double High { get; set; }
    public double Low { get; set; }
    public double Close { get; set; }
    public ulong Volume { get; set; }
    public ulong BuyVolume { get; set; }
    public ulong SellVolume { get; set; }
    public long Delta => (long)BuyVolume - (long)SellVolume;
    public ulong TradeCount { get; set; }
    public string Symbol { get; set; } = "";

    public bool IsBullish => Close > Open;
    public double BodySize => Math.Abs(Close - Open);
    public double Range => High - Low;
}
