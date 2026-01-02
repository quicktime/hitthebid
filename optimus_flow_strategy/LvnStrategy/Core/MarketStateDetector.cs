using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Market state classification: Balanced (rotational) vs Imbalanced (trending).
/// Matches Rust implementation from src/trading_core/market_state.rs
/// </summary>
public enum MarketState
{
    /// <summary>Price consolidating, no clear direction</summary>
    Balanced,

    /// <summary>Price trending with momentum</summary>
    Imbalanced
}

/// <summary>
/// Detects market state (balanced vs imbalanced) from recent bar data.
/// Imbalanced markets are required for LVN retest signals.
/// </summary>
public class MarketStateDetector
{
    /// <summary>Number of bars to analyze for market state</summary>
    private const int LookbackBars = 60;

    /// <summary>Threshold for cumulative delta to indicate imbalance</summary>
    private const long DeltaThreshold = 200;

    /// <summary>Multiplier for ATR-based range detection</summary>
    private const double AtrMultiplier = 2.0;

    private readonly Queue<Bar> _recentBars = new();
    private double _atr;

    /// <summary>
    /// Current market state
    /// </summary>
    public MarketState CurrentState { get; private set; } = MarketState.Balanced;

    /// <summary>
    /// Process a new bar and update market state
    /// </summary>
    public MarketState ProcessBar(Bar bar)
    {
        _recentBars.Enqueue(bar);

        // Maintain rolling window
        while (_recentBars.Count > LookbackBars)
        {
            _recentBars.Dequeue();
        }

        if (_recentBars.Count < LookbackBars)
        {
            // Not enough data yet
            CurrentState = MarketState.Balanced;
            return CurrentState;
        }

        // Calculate metrics
        var bars = _recentBars.ToList();
        var rangeHigh = bars.Max(b => b.High);
        var rangeLow = bars.Min(b => b.Low);
        var range = rangeHigh - rangeLow;
        var cumulativeDelta = bars.Sum(b => b.Delta);

        // Update ATR (simple average true range)
        UpdateAtr(bars);

        // Determine market state
        var isImbalanced = false;

        // Condition 1: Range exceeds ATR threshold
        if (range > _atr * AtrMultiplier)
        {
            isImbalanced = true;
        }

        // Condition 2: Cumulative delta exceeds threshold
        if (Math.Abs(cumulativeDelta) > DeltaThreshold)
        {
            isImbalanced = true;
        }

        CurrentState = isImbalanced ? MarketState.Imbalanced : MarketState.Balanced;
        return CurrentState;
    }

    private void UpdateAtr(List<Bar> bars)
    {
        if (bars.Count < 2) return;

        double sumTr = 0;
        for (int i = 1; i < bars.Count; i++)
        {
            var current = bars[i];
            var previous = bars[i - 1];

            var tr = Math.Max(
                current.High - current.Low,
                Math.Max(
                    Math.Abs(current.High - previous.Close),
                    Math.Abs(current.Low - previous.Close)
                )
            );
            sumTr += tr;
        }

        _atr = sumTr / (bars.Count - 1);
    }

    /// <summary>
    /// Get the current Average True Range
    /// </summary>
    public double GetAtr() => _atr;

    /// <summary>
    /// Get cumulative delta over the lookback period
    /// </summary>
    public long GetCumulativeDelta()
    {
        return _recentBars.Sum(b => b.Delta);
    }

    /// <summary>
    /// Reset the detector state
    /// </summary>
    public void Reset()
    {
        _recentBars.Clear();
        _atr = 0;
        CurrentState = MarketState.Balanced;
    }
}
