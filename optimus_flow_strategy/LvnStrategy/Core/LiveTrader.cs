using LvnStrategy.Config;
using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Manages live trading position, stop management, and P&L tracking.
/// Coordinates StateMachine and SignalGenerator for full trading lifecycle.
/// Matches Rust implementation from src/trading_core/trader.rs
/// </summary>
public class LiveTrader
{
    private readonly TradingConfig _config;
    private readonly StateMachine _stateMachine;
    private readonly SignalGenerator _signalGenerator;
    private readonly BarAggregator _barAggregator;

    private OpenPosition? _position;
    private int _barCount;
    private int _dailyLosses;
    private double _dailyPnl;
    private bool _isTradingAllowed = true;

    /// <summary>Current position (null if flat)</summary>
    public OpenPosition? Position => _position;

    /// <summary>Current trading state</summary>
    public TradingState CurrentState => _stateMachine.CurrentState;

    /// <summary>Whether currently in a position</summary>
    public bool HasPosition => _position != null;

    /// <summary>Daily P&L in points</summary>
    public double DailyPnl => _dailyPnl;

    /// <summary>Daily losses count</summary>
    public int DailyLosses => _dailyLosses;

    /// <summary>Whether trading is allowed (not stopped due to risk limits)</summary>
    public bool IsTradingAllowed => _isTradingAllowed;

    public event EventHandler<TradeAction>? OnTradeAction;
    public event EventHandler<StateTransition>? OnStateTransition;

    public LiveTrader(TradingConfig config)
    {
        _config = config;
        _stateMachine = new StateMachine(config);
        _signalGenerator = new SignalGenerator(config);
        _barAggregator = new BarAggregator(config.Symbol);

        _stateMachine.OnStateTransition += (_, t) => OnStateTransition?.Invoke(this, t);
    }

    /// <summary>
    /// Set daily levels for breakout detection
    /// </summary>
    public void SetDailyLevels(DailyLevels levels)
    {
        _stateMachine.DailyLevels = levels;
    }

    /// <summary>
    /// Check if currently profiling an impulse
    /// </summary>
    public bool IsProfilingImpulse => _stateMachine.IsProfilingImpulse;

    /// <summary>
    /// Process a trade (used during impulse profiling)
    /// </summary>
    public void ProcessTrade(Trade trade)
    {
        _stateMachine.ProcessTrade(trade);
    }

    /// <summary>
    /// Process a bar and return any trade action
    /// </summary>
    public TradeAction? ProcessBar(Bar bar)
    {
        _barCount++;

        // Check trading hours
        if (!_config.IsWithinTradingHours(bar.Timestamp))
        {
            if (_position != null)
            {
                return FlattenPosition("End of trading hours");
            }
            return null;
        }

        // Check risk limits
        if (!_isTradingAllowed)
        {
            if (_position != null)
            {
                return FlattenPosition("Risk limits exceeded");
            }
            return null;
        }

        // If in position, manage it
        if (_position != null)
        {
            return ManagePosition(bar);
        }

        // Process state machine
        var transition = _stateMachine.ProcessBar(bar);

        // If we just entered hunting, add LVNs to signal generator
        if (transition is StateTransition.ImpulseComplete complete)
        {
            _signalGenerator.AddLevels(_stateMachine.ActiveLvns);
        }

        // If hunting, check for signals
        if (_stateMachine.IsHunting)
        {
            var signal = _signalGenerator.ProcessBar(bar);
            if (signal != null)
            {
                return EnterPosition(signal);
            }
        }

        return null;
    }

    private TradeAction? ManagePosition(Bar bar)
    {
        if (_position == null) return null;

        // Update position tracking
        _position.BarCount++;
        _position.HighestPrice = Math.Max(_position.HighestPrice, bar.High);
        _position.LowestPrice = Math.Min(_position.LowestPrice, bar.Low);

        // Check take profit
        if (_position.IsTakeProfitHit(bar.Close))
        {
            return ExitPosition(bar.Close, "Take Profit");
        }

        // Check stop loss
        if (_position.IsStopHit(bar.Close))
        {
            return ExitPosition(_position.TrailingStop, "Stop Loss");
        }

        // Update trailing stop
        if (_position.ShouldUpdateTrailingStop(bar.Close, _config.TrailingStop))
        {
            var newStop = _position.Direction == Direction.Long
                ? bar.Close - _config.TrailingStop
                : bar.Close + _config.TrailingStop;

            _position.TrailingStop = newStop;

            var action = new TradeAction.UpdateStop(newStop);
            OnTradeAction?.Invoke(this, action);
            return action;
        }

        return null;
    }

    private TradeAction EnterPosition(Signal signal)
    {
        _position = new OpenPosition
        {
            Direction = signal.Direction,
            EntryPrice = signal.EntryPrice,
            EntryTime = signal.Timestamp,
            LevelPrice = signal.TriggeringLevel.Price,
            InitialStop = signal.StopPrice,
            TakeProfit = signal.TargetPrice,
            TrailingStop = signal.StopPrice,
            HighestPrice = signal.EntryPrice,
            LowestPrice = signal.EntryPrice,
            BarCount = 0,
            Contracts = signal.Contracts
        };

        var action = new TradeAction.Enter(
            signal.Direction,
            signal.EntryPrice,
            signal.StopPrice,
            signal.TargetPrice,
            signal.Contracts
        );

        OnTradeAction?.Invoke(this, action);
        return action;
    }

    private TradeAction ExitPosition(double exitPrice, string reason)
    {
        if (_position == null)
            throw new InvalidOperationException("No position to exit");

        var pnl = _position.Direction == Direction.Long
            ? exitPrice - _position.EntryPrice
            : _position.EntryPrice - exitPrice;

        // Apply slippage and commission
        pnl -= _config.Slippage * 2; // Entry and exit slippage
        pnl -= _config.Commission / _config.PointValue; // Convert commission to points

        // Update daily stats
        _dailyPnl += pnl;
        if (pnl < 0)
        {
            _dailyLosses++;

            // Check daily loss limits
            if (_dailyLosses >= _config.MaxDailyLosses)
            {
                _isTradingAllowed = false;
            }
            if (_dailyPnl <= -_config.DailyLossLimit)
            {
                _isTradingAllowed = false;
            }
        }

        var action = new TradeAction.Exit(
            _position.Direction,
            exitPrice,
            pnl,
            reason
        );

        _position = null;

        // Trigger state machine reset
        _stateMachine.TriggerReset();
        _signalGenerator.ClearLevels();

        OnTradeAction?.Invoke(this, action);
        return action;
    }

    private TradeAction FlattenPosition(string reason)
    {
        if (_position == null)
            return new TradeAction.FlattenAll(reason);

        // Close at current stop (worst case)
        var exitAction = ExitPosition(_position.TrailingStop, reason);

        return new TradeAction.FlattenAll(reason);
    }

    /// <summary>
    /// Reset daily statistics (call at start of new trading day)
    /// </summary>
    public void ResetDailyStats()
    {
        _dailyLosses = 0;
        _dailyPnl = 0;
        _isTradingAllowed = true;
    }

    /// <summary>
    /// Get summary statistics
    /// </summary>
    public TradingStats GetStats()
    {
        return new TradingStats
        {
            DailyPnl = _dailyPnl,
            DailyLosses = _dailyLosses,
            BarCount = _barCount,
            IsTradingAllowed = _isTradingAllowed,
            CurrentState = _stateMachine.CurrentState.ToString(),
            TrackedLevelCount = _signalGenerator.GetTrackedLevelCount(),
            ArmedLevelCount = _signalGenerator.GetArmedLevelCount()
        };
    }
}

/// <summary>
/// Trading statistics summary
/// </summary>
public class TradingStats
{
    public double DailyPnl { get; set; }
    public int DailyLosses { get; set; }
    public int BarCount { get; set; }
    public bool IsTradingAllowed { get; set; }
    public string CurrentState { get; set; } = "";
    public int TrackedLevelCount { get; set; }
    public int ArmedLevelCount { get; set; }
}
