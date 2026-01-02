using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Aggregates tick trades into 1-second OHLCV bars with delta.
/// Matches Rust struct from src/pipeline/databento_ib_live.rs:37-135
/// </summary>
public class BarAggregator
{
    private readonly string _symbol;
    private BarBuilder? _currentBar;

    public BarAggregator(string symbol)
    {
        _symbol = symbol;
    }

    /// <summary>
    /// Process a trade and return completed bar if a new second started
    /// </summary>
    public Bar? ProcessTrade(DateTime timestamp, double price, ulong size, bool isBuy)
    {
        // Get unix timestamp (seconds since epoch)
        var second = new DateTimeOffset(timestamp).ToUnixTimeSeconds();

        if (_currentBar is not null)
        {
            var barSecond = new DateTimeOffset(_currentBar.Timestamp).ToUnixTimeSeconds();
            if (second > barSecond)
            {
                // New second - complete current bar and start new one
                var completed = _currentBar.ToBar(_symbol);
                _currentBar = new BarBuilder(timestamp, price, size, isBuy);
                return completed;
            }
            else
            {
                // Same second - add to current bar
                _currentBar.AddTrade(price, size, isBuy);
                return null;
            }
        }
        else
        {
            // First trade
            _currentBar = new BarBuilder(timestamp, price, size, isBuy);
            return null;
        }
    }

    /// <summary>
    /// Get the current partial bar (if any)
    /// </summary>
    public Bar? GetCurrentBar()
    {
        return _currentBar?.ToBar(_symbol);
    }

    /// <summary>
    /// Flush the current bar even if the second hasn't completed
    /// </summary>
    public Bar? Flush()
    {
        if (_currentBar is null) return null;

        var bar = _currentBar.ToBar(_symbol);
        _currentBar = null;
        return bar;
    }
}

/// <summary>
/// Internal helper to build a bar from trades
/// </summary>
internal class BarBuilder
{
    public DateTime Timestamp { get; }
    private double _open;
    private double _high;
    private double _low;
    private double _close;
    private ulong _volume;
    private ulong _buyVolume;
    private ulong _sellVolume;
    private ulong _tradeCount;

    public BarBuilder(DateTime timestamp, double price, ulong size, bool isBuy)
    {
        Timestamp = timestamp;
        _open = price;
        _high = price;
        _low = price;
        _close = price;
        _volume = size;

        if (isBuy)
        {
            _buyVolume = size;
            _sellVolume = 0;
        }
        else
        {
            _buyVolume = 0;
            _sellVolume = size;
        }
        _tradeCount = 1;
    }

    public void AddTrade(double price, ulong size, bool isBuy)
    {
        _high = Math.Max(_high, price);
        _low = Math.Min(_low, price);
        _close = price;
        _volume += size;

        if (isBuy)
            _buyVolume += size;
        else
            _sellVolume += size;

        _tradeCount++;
    }

    public Bar ToBar(string symbol)
    {
        return new Bar
        {
            Timestamp = Timestamp,
            Open = _open,
            High = _high,
            Low = _low,
            Close = _close,
            Volume = _volume,
            BuyVolume = _buyVolume,
            SellVolume = _sellVolume,
            TradeCount = _tradeCount,
            Symbol = symbol
        };
    }
}
