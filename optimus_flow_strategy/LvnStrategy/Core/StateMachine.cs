using LvnStrategy.Config;
using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// State of the trading state machine.
/// Matches Rust enum from src/trading_core/state_machine.rs
/// </summary>
public enum TradingState
{
    /// <summary>Waiting for price to break a significant level</summary>
    WaitingForBreakout,

    /// <summary>Breakout detected, profiling the impulse leg</summary>
    ProfilingImpulse,

    /// <summary>Impulse complete, hunting for LVN retest</summary>
    Hunting,

    /// <summary>Resetting for next cycle</summary>
    Reset
}

/// <summary>
/// Type of breakout level
/// </summary>
public enum BreakoutLevel
{
    PDH, // Prior Day High
    PDL, // Prior Day Low
    ONH, // Overnight High
    ONL, // Overnight Low
    VAH, // Value Area High
    VAL  // Value Area Low
}

/// <summary>
/// State transition events for logging and debugging
/// </summary>
public abstract record StateTransition
{
    public record BreakoutDetected(BreakoutLevel Level, ImpulseDirection Direction, double Price) : StateTransition;
    public record ImpulseComplete(Guid ImpulseId, int LvnCount, ImpulseDirection Direction) : StateTransition;
    public record ImpulseInvalid(string Reason) : StateTransition;
    public record HuntingTimeout : StateTransition;
    public record ResetComplete : StateTransition;
}

/// <summary>
/// Active impulse being profiled
/// </summary>
public class ActiveImpulse
{
    public Guid Id { get; set; } = Guid.NewGuid();
    public ImpulseDirection Direction { get; set; }
    public BreakoutLevel BrokenLevel { get; set; }
    public ImpulseBuilder Builder { get; set; } = new();
    public List<Trade> Trades { get; set; } = new();
    public int StartBarIndex { get; set; }
}

/// <summary>
/// Trading State Machine for Real-Time LVN Strategy.
///
/// Implements the CORE trading strategy:
/// 1. WAITING_FOR_BREAKOUT - Wait for price to break significant level (PDH/PDL, ONH/ONL, VAH/VAL)
/// 2. PROFILING_IMPULSE - Track the impulse leg in real-time as it forms
/// 3. HUNTING - Wait for pullback to LVN with delta confirmation
/// 4. RESET - After trade, clear ALL LVNs from that impulse and return to waiting
///
/// Matches Rust implementation from src/trading_core/state_machine.rs
/// </summary>
public class StateMachine
{
    private readonly TradingConfig _config;

    public TradingState CurrentState { get; private set; } = TradingState.WaitingForBreakout;
    public DailyLevels? DailyLevels { get; set; }
    public ActiveImpulse? CurrentImpulse { get; private set; }
    public List<LvnLevel> ActiveLvns { get; private set; } = new();

    private int _barCount;
    private int _huntingStartBar;
    private readonly List<StateTransition> _transitionHistory = new();

    public event EventHandler<StateTransition>? OnStateTransition;

    public StateMachine(TradingConfig config)
    {
        _config = config;
    }

    /// <summary>
    /// Process a new bar and return any state transitions
    /// </summary>
    public StateTransition? ProcessBar(Bar bar)
    {
        _barCount++;

        return CurrentState switch
        {
            TradingState.WaitingForBreakout => ProcessWaitingForBreakout(bar),
            TradingState.ProfilingImpulse => ProcessProfilingImpulse(bar),
            TradingState.Hunting => ProcessHunting(bar),
            TradingState.Reset => ProcessReset(bar),
            _ => null
        };
    }

    /// <summary>
    /// Process a trade (used during impulse profiling for LVN extraction)
    /// </summary>
    public void ProcessTrade(Trade trade)
    {
        if (CurrentState == TradingState.ProfilingImpulse && CurrentImpulse != null)
        {
            CurrentImpulse.Trades.Add(trade);
        }
    }

    /// <summary>
    /// Check if currently profiling an impulse
    /// </summary>
    public bool IsProfilingImpulse => CurrentState == TradingState.ProfilingImpulse;

    /// <summary>
    /// Check if currently hunting for LVN retest
    /// </summary>
    public bool IsHunting => CurrentState == TradingState.Hunting;

    private StateTransition? ProcessWaitingForBreakout(Bar bar)
    {
        if (DailyLevels == null) return null;

        var breakout = CheckBreakout(bar.Close);
        if (breakout == null) return null;

        var (level, direction) = breakout.Value;

        // Start profiling impulse
        CurrentState = TradingState.ProfilingImpulse;
        CurrentImpulse = new ActiveImpulse
        {
            Direction = direction,
            BrokenLevel = level,
            StartBarIndex = _barCount,
            Builder = new ImpulseBuilder(direction, bar)
        };

        var transition = new StateTransition.BreakoutDetected(level, direction, bar.Close);
        RecordTransition(transition);
        return transition;
    }

    private StateTransition? ProcessProfilingImpulse(Bar bar)
    {
        if (CurrentImpulse == null) return null;

        // Add bar to impulse builder
        CurrentImpulse.Builder.AddBar(bar);

        var barsInImpulse = _barCount - CurrentImpulse.StartBarIndex;

        // Check for timeout
        if (barsInImpulse > _config.MaxImpulseBars)
        {
            var transition = new StateTransition.ImpulseInvalid("Impulse profiling timeout");
            TransitionToReset(transition);
            return transition;
        }

        // Check for retrace invalidation
        if (CurrentImpulse.Builder.GetRetraceRatio() > _config.MaxRetraceRatio)
        {
            var transition = new StateTransition.ImpulseInvalid("Impulse retraced too much");
            TransitionToReset(transition);
            return transition;
        }

        // Check if impulse is complete (sufficient size and score)
        var impulseSize = CurrentImpulse.Builder.GetImpulseSize();
        var impulseScore = CurrentImpulse.Builder.CalculateScore();

        if (impulseSize >= _config.MinImpulseSize && impulseScore >= _config.MinImpulseScore)
        {
            // Extract LVNs and transition to hunting
            var lvns = ExtractLvns(CurrentImpulse);
            ActiveLvns.AddRange(lvns);

            CurrentState = TradingState.Hunting;
            _huntingStartBar = _barCount;

            var transition = new StateTransition.ImpulseComplete(
                CurrentImpulse.Id,
                lvns.Count,
                CurrentImpulse.Direction
            );
            RecordTransition(transition);
            return transition;
        }

        return null;
    }

    private StateTransition? ProcessHunting(Bar bar)
    {
        var barsHunting = _barCount - _huntingStartBar;

        // Check for hunting timeout
        if (barsHunting > _config.MaxHuntingBars)
        {
            var transition = new StateTransition.HuntingTimeout();
            TransitionToReset(transition);
            return transition;
        }

        // The actual signal detection is handled by SignalGenerator
        // State machine just tracks the hunting phase

        return null;
    }

    private StateTransition? ProcessReset(Bar bar)
    {
        // Clear all state and return to waiting
        CurrentImpulse = null;
        ActiveLvns.Clear();
        CurrentState = TradingState.WaitingForBreakout;

        var transition = new StateTransition.ResetComplete();
        RecordTransition(transition);
        return transition;
    }

    /// <summary>
    /// Force transition to reset state (after trade completion)
    /// </summary>
    public void TriggerReset()
    {
        CurrentState = TradingState.Reset;
    }

    private void TransitionToReset(StateTransition transition)
    {
        CurrentState = TradingState.Reset;
        RecordTransition(transition);
    }

    private void RecordTransition(StateTransition transition)
    {
        _transitionHistory.Add(transition);
        OnStateTransition?.Invoke(this, transition);
    }

    private (BreakoutLevel Level, ImpulseDirection Direction)? CheckBreakout(double price)
    {
        if (DailyLevels == null) return null;

        var threshold = _config.BreakoutThreshold;

        // Check PDH/PDL first (most significant)
        if (price > DailyLevels.Pdh + threshold)
            return (BreakoutLevel.PDH, ImpulseDirection.Up);
        if (price < DailyLevels.Pdl - threshold)
            return (BreakoutLevel.PDL, ImpulseDirection.Down);

        // Check ONH/ONL
        if (DailyLevels.Onh > 0 && price > DailyLevels.Onh + threshold)
            return (BreakoutLevel.ONH, ImpulseDirection.Up);
        if (DailyLevels.Onl > 0 && price < DailyLevels.Onl - threshold)
            return (BreakoutLevel.ONL, ImpulseDirection.Down);

        // Check VAH/VAL
        if (price > DailyLevels.Vah + threshold)
            return (BreakoutLevel.VAH, ImpulseDirection.Up);
        if (price < DailyLevels.Val - threshold)
            return (BreakoutLevel.VAL, ImpulseDirection.Down);

        return null;
    }

    private List<LvnLevel> ExtractLvns(ActiveImpulse impulse)
    {
        // Delegate to LvnExtractor
        return LvnExtractor.ExtractFromTrades(
            impulse.Trades,
            impulse.Direction,
            impulse.Id,
            _config.LvnBucketSize,
            _config.LvnThresholdRatio
        );
    }

    public IReadOnlyList<StateTransition> GetTransitionHistory() => _transitionHistory.AsReadOnly();
}
