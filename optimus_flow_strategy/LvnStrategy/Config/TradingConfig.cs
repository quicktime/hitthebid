namespace LvnStrategy.Config;

/// <summary>
/// Main trading configuration combining LiveConfig, StateMachineConfig, and LvnRetestConfig.
/// Matches Rust structs from src/trading_core/trader.rs, state_machine.rs, and lvn_retest.rs
/// </summary>
public class TradingConfig
{
    // ══════════════════════════════════════════════════════════════════
    // Basic Trading Settings (from LiveConfig)
    // ══════════════════════════════════════════════════════════════════

    /// <summary>Symbol to trade (e.g., "MNQ" for Micro Nasdaq)</summary>
    public string Symbol { get; set; } = "MNQ";

    /// <summary>Exchange (e.g., "CME")</summary>
    public string Exchange { get; set; } = "CME";

    /// <summary>Number of contracts to trade</summary>
    public int Contracts { get; set; } = 1;

    /// <summary>Cache directory for daily levels</summary>
    public string CacheDir { get; set; } = "cache";

    /// <summary>Take profit in points (0 = trailing stop only)</summary>
    public double TakeProfit { get; set; } = 0;

    /// <summary>Trailing stop distance in points</summary>
    public double TrailingStop { get; set; } = 4.0;

    /// <summary>Stop buffer beyond LVN level in points</summary>
    public double StopBuffer { get; set; } = 2.0;

    /// <summary>Trading start hour (ET, 24h format)</summary>
    public int StartHour { get; set; } = 9;

    /// <summary>Trading start minute</summary>
    public int StartMinute { get; set; } = 30;

    /// <summary>Trading end hour (ET, 24h format)</summary>
    public int EndHour { get; set; } = 16;

    /// <summary>Trading end minute</summary>
    public int EndMinute { get; set; } = 0;

    /// <summary>Minimum delta for absorption signal</summary>
    public long MinDelta { get; set; } = 150;

    /// <summary>Maximum LVN volume ratio (lower = thinner = better)</summary>
    public double MaxLvnRatio { get; set; } = 0.15;

    /// <summary>Level tolerance in points for "at level" detection</summary>
    public double LevelTolerance { get; set; } = 2.0;

    /// <summary>Starting balance for P&L tracking</summary>
    public double StartingBalance { get; set; } = 500;

    /// <summary>Max losing trades per day before stopping</summary>
    public int MaxDailyLosses { get; set; } = 3;

    /// <summary>Daily P&L loss limit in points</summary>
    public double DailyLossLimit { get; set; } = 50;

    /// <summary>Point value per tick (MNQ = $0.50, NQ = $5)</summary>
    public double PointValue { get; set; } = 0.50;

    /// <summary>Slippage per trade in points</summary>
    public double Slippage { get; set; } = 0;

    /// <summary>Commission per round-trip in dollars</summary>
    public double Commission { get; set; } = 0.50;

    // ══════════════════════════════════════════════════════════════════
    // State Machine Settings (from StateMachineConfig)
    // ══════════════════════════════════════════════════════════════════

    /// <summary>Points beyond level to confirm breakout</summary>
    public double BreakoutThreshold { get; set; } = 2.0;

    /// <summary>Maximum bars for impulse profiling before timeout (1s bars)</summary>
    public int MaxImpulseBars { get; set; } = 300; // 5 minutes

    /// <summary>Minimum points for a valid impulse</summary>
    public double MinImpulseSize { get; set; } = 25.0;

    /// <summary>Maximum bars to hunt for retest before timeout (1s bars)</summary>
    public int MaxHuntingBars { get; set; } = 600; // 10 minutes

    /// <summary>Minimum impulse score (out of 5) to qualify</summary>
    public int MinImpulseScore { get; set; } = 4;

    /// <summary>Maximum retrace ratio before impulse is invalidated</summary>
    public double MaxRetraceRatio { get; set; } = 0.7;

    /// <summary>Minimum bars before considering switching to a new breakout</summary>
    public int MinBarsBeforeSwitch { get; set; } = 60;

    // ══════════════════════════════════════════════════════════════════
    // LVN Retest Settings (from LvnRetestConfig)
    // ══════════════════════════════════════════════════════════════════

    /// <summary>Distance price must move away before level is "armed" (points)</summary>
    public double RetestDistance { get; set; } = 8.0;

    /// <summary>Maximum range for absorption signal (points)</summary>
    public double MaxRangeForAbsorption { get; set; } = 1.5;

    /// <summary>Maximum hold time in bars (1-second bars)</summary>
    public int MaxHoldBars { get; set; } = 300;

    /// <summary>Only trade during RTH</summary>
    public bool RthOnly { get; set; } = true;

    /// <summary>Minimum bars between trades (global cooldown)</summary>
    public int CooldownBars { get; set; } = 60; // 1 minute

    /// <summary>Cooldown per level after trading it</summary>
    public int LevelCooldownBars { get; set; } = 600; // 10 minutes

    /// <summary>Only use same-day LVNs</summary>
    public bool SameDayOnly { get; set; } = false;

    /// <summary>Require multiple bars of absorption</summary>
    public int MinAbsorptionBars { get; set; } = 1;

    // ══════════════════════════════════════════════════════════════════
    // LVN Extraction Settings
    // ══════════════════════════════════════════════════════════════════

    /// <summary>Volume profile bucket size in points</summary>
    public double LvnBucketSize { get; set; } = 0.5;

    /// <summary>Volume threshold ratio for LVN qualification</summary>
    public double LvnThresholdRatio { get; set; } = 0.15;

    // ══════════════════════════════════════════════════════════════════
    // Helper Methods
    // ══════════════════════════════════════════════════════════════════

    /// <summary>
    /// Check if current time is within trading hours (Eastern Time)
    /// </summary>
    public bool IsWithinTradingHours(DateTime utcNow)
    {
        // Convert UTC to Eastern Time
        var eastern = TimeZoneInfo.FindSystemTimeZoneById("Eastern Standard Time");
        var etNow = TimeZoneInfo.ConvertTimeFromUtc(utcNow, eastern);

        var startTime = new TimeSpan(StartHour, StartMinute, 0);
        var endTime = new TimeSpan(EndHour, EndMinute, 0);
        var currentTime = etNow.TimeOfDay;

        return currentTime >= startTime && currentTime <= endTime;
    }

    /// <summary>
    /// Create default MNQ configuration
    /// </summary>
    public static TradingConfig DefaultMnq()
    {
        return new TradingConfig
        {
            Symbol = "MNQ",
            Exchange = "CME",
            Contracts = 1,
            PointValue = 0.50,  // $0.50 per 0.25 tick = $2.00 per point
            Commission = 0.50,  // $0.25/side via Optimus
            TakeProfit = 0,
            TrailingStop = 4.0,
            StopBuffer = 2.0,
            StartHour = 9,
            StartMinute = 30,
            EndHour = 16,
            EndMinute = 0
        };
    }

    /// <summary>
    /// Create default NQ configuration
    /// </summary>
    public static TradingConfig DefaultNq()
    {
        return new TradingConfig
        {
            Symbol = "NQ",
            Exchange = "CME",
            Contracts = 1,
            PointValue = 5.0,   // $5.00 per 0.25 tick = $20.00 per point
            Commission = 2.50,  // Approximate commission
            TakeProfit = 0,
            TrailingStop = 4.0,
            StopBuffer = 2.0,
            StartHour = 9,
            StartMinute = 30,
            EndHour = 16,
            EndMinute = 0
        };
    }
}
