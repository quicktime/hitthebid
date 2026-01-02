using LvnStrategy.Config;
using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Level state for tracking LVN progression through touch/arm/retest cycle
/// </summary>
public enum LevelState
{
    /// <summary>Level has not been touched since extraction</summary>
    Untouched,

    /// <summary>Price touched the level but moved away</summary>
    Touched,

    /// <summary>Price moved far enough away to arm the level for retest</summary>
    Armed,

    /// <summary>Price is currently retesting the armed level</summary>
    Retesting
}

/// <summary>
/// Tracked LVN level with state
/// </summary>
public class TrackedLevel
{
    public LvnLevel Level { get; set; } = null!;
    public LevelState State { get; set; } = LevelState.Untouched;
    public int LastTouchBar { get; set; }
    public int CooldownUntilBar { get; set; }
    public double FarthestPrice { get; set; }
}

/// <summary>
/// Signal output from the generator
/// </summary>
public class Signal
{
    public required Direction Direction { get; init; }
    public required double EntryPrice { get; init; }
    public required double StopPrice { get; init; }
    public required double TargetPrice { get; init; }
    public required int Contracts { get; init; }
    public required LvnLevel TriggeringLevel { get; init; }
    public required long Delta { get; init; }
    public required DateTime Timestamp { get; init; }
}

/// <summary>
/// Generates trading signals when price retests LVN levels with absorption.
/// Implements level state tracking (Untouched → Touched → Armed → Retesting).
/// Matches Rust implementation from src/trading_core/lvn_retest.rs
/// </summary>
public class SignalGenerator
{
    private readonly TradingConfig _config;
    private readonly MarketStateDetector _marketStateDetector = new();
    private readonly List<TrackedLevel> _trackedLevels = new();

    private int _barCount;
    private int _globalCooldownUntil;
    private int _lastSignalBar;

    public SignalGenerator(TradingConfig config)
    {
        _config = config;
    }

    /// <summary>
    /// Add LVN levels to track
    /// </summary>
    public void AddLevels(IEnumerable<LvnLevel> levels)
    {
        foreach (var level in levels)
        {
            _trackedLevels.Add(new TrackedLevel
            {
                Level = level,
                State = LevelState.Untouched
            });
        }
    }

    /// <summary>
    /// Clear all tracked levels
    /// </summary>
    public void ClearLevels()
    {
        _trackedLevels.Clear();
    }

    /// <summary>
    /// Clear levels from a specific impulse
    /// </summary>
    public void ClearLevelsForImpulse(Guid impulseId)
    {
        _trackedLevels.RemoveAll(t => t.Level.ImpulseId == impulseId);
    }

    /// <summary>
    /// Process a bar and check for signals
    /// </summary>
    public Signal? ProcessBar(Bar bar)
    {
        _barCount++;

        // Update market state
        var marketState = _marketStateDetector.ProcessBar(bar);

        // Update level states
        UpdateLevelStates(bar);

        // Check cooldown
        if (_barCount < _globalCooldownUntil)
            return null;

        // Only generate signals in imbalanced market
        if (marketState != MarketState.Imbalanced)
            return null;

        // Check for signals
        return CheckForSignal(bar);
    }

    private void UpdateLevelStates(Bar bar)
    {
        foreach (var tracked in _trackedLevels)
        {
            // Skip if on cooldown
            if (_barCount < tracked.CooldownUntilBar)
                continue;

            var distance = Math.Abs(bar.Close - tracked.Level.Price);
            var atLevel = distance <= _config.LevelTolerance;

            switch (tracked.State)
            {
                case LevelState.Untouched:
                    if (atLevel)
                    {
                        tracked.State = LevelState.Touched;
                        tracked.LastTouchBar = _barCount;
                        tracked.FarthestPrice = bar.Close;
                    }
                    break;

                case LevelState.Touched:
                    if (!atLevel)
                    {
                        // Track how far price moved
                        var movedAway = tracked.Level.ImpulseDirection == ImpulseDirection.Up
                            ? bar.Close > tracked.FarthestPrice
                            : bar.Close < tracked.FarthestPrice;

                        if (movedAway)
                        {
                            tracked.FarthestPrice = bar.Close;
                        }

                        // Check if far enough to arm
                        var distanceFromLevel = Math.Abs(tracked.FarthestPrice - tracked.Level.Price);
                        if (distanceFromLevel >= _config.RetestDistance)
                        {
                            tracked.State = LevelState.Armed;
                        }
                    }
                    break;

                case LevelState.Armed:
                    if (atLevel)
                    {
                        tracked.State = LevelState.Retesting;
                    }
                    break;

                case LevelState.Retesting:
                    if (!atLevel)
                    {
                        // Moved away from level without triggering
                        tracked.State = LevelState.Armed;
                    }
                    break;
            }
        }
    }

    private Signal? CheckForSignal(Bar bar)
    {
        // Find levels that are being retested
        var retestingLevels = _trackedLevels
            .Where(t => t.State == LevelState.Retesting)
            .Where(t => _barCount >= t.CooldownUntilBar)
            .ToList();

        foreach (var tracked in retestingLevels)
        {
            // Check for absorption: heavy delta with minimal range
            var hasAbsorption = Math.Abs(bar.Delta) >= _config.MinDelta
                                && bar.Range <= _config.MaxRangeForAbsorption;

            if (!hasAbsorption)
                continue;

            // Check delta direction matches impulse direction
            var correctDelta = tracked.Level.ImpulseDirection == ImpulseDirection.Up
                ? bar.Delta > 0  // Buyers absorbing selling at LVN
                : bar.Delta < 0; // Sellers absorbing buying at LVN

            if (!correctDelta)
                continue;

            // Generate signal!
            var direction = tracked.Level.ImpulseDirection == ImpulseDirection.Up
                ? Direction.Long
                : Direction.Short;

            var stopPrice = direction == Direction.Long
                ? tracked.Level.Price - _config.StopBuffer
                : tracked.Level.Price + _config.StopBuffer;

            var targetPrice = _config.TakeProfit > 0
                ? (direction == Direction.Long
                    ? bar.Close + _config.TakeProfit
                    : bar.Close - _config.TakeProfit)
                : 0; // Trailing stop only

            // Apply cooldowns
            _globalCooldownUntil = _barCount + _config.CooldownBars;
            tracked.CooldownUntilBar = _barCount + _config.LevelCooldownBars;
            tracked.State = LevelState.Touched; // Reset to touched
            _lastSignalBar = _barCount;

            return new Signal
            {
                Direction = direction,
                EntryPrice = bar.Close,
                StopPrice = stopPrice,
                TargetPrice = targetPrice,
                Contracts = _config.Contracts,
                TriggeringLevel = tracked.Level,
                Delta = bar.Delta,
                Timestamp = bar.Timestamp
            };
        }

        return null;
    }

    /// <summary>
    /// Get current market state
    /// </summary>
    public MarketState GetMarketState() => _marketStateDetector.CurrentState;

    /// <summary>
    /// Get number of tracked levels
    /// </summary>
    public int GetTrackedLevelCount() => _trackedLevels.Count;

    /// <summary>
    /// Get number of armed levels
    /// </summary>
    public int GetArmedLevelCount() => _trackedLevels.Count(t => t.State == LevelState.Armed);

    /// <summary>
    /// Check if in global cooldown
    /// </summary>
    public bool IsInCooldown => _barCount < _globalCooldownUntil;
}
