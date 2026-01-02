using LvnStrategy.Config;
using LvnStrategy.Core;
using LvnStrategy.Data;
using LvnStrategy.Execution;
using LvnStrategy.Models;

namespace LvnStrategy.Strategy;

/// <summary>
/// Main LVN Trading Strategy for Optimus Flow (Quantower).
///
/// This is the entry point that coordinates all components:
/// - DatabentoClient: Live market data
/// - BarAggregator: Tick to 1-second bars
/// - LiveTrader: Trading logic (state machine, signals, positions)
/// - QuantowerExecutor: Order execution
///
/// To use in Optimus Flow:
/// 1. Create a new Strategy project in Algo Lab
/// 2. Reference this assembly
/// 3. Inherit from Quantower Strategy base class
/// 4. Call Initialize() in OnRun(), Stop() in OnStop()
/// </summary>
public class LvnTradingStrategy : IAsyncDisposable
{
    private readonly TradingConfig _config;
    private readonly LiveTrader _trader;
    private readonly QuantowerExecutor _executor;
    private readonly LevelCache _levelCache;

    private DatabentoClient? _dataClient;
    private BarAggregator? _barAggregator;
    private CancellationTokenSource? _cts;
    private Task? _tradingTask;

    private bool _isRunning;
    private int _signalCount;
    private int _wins;
    private int _losses;

    public event EventHandler<string>? OnLog;
    public event EventHandler<TradeAction>? OnTradeAction;

    public LvnTradingStrategy(TradingConfig config)
    {
        _config = config;
        _trader = new LiveTrader(config);
        _executor = new QuantowerExecutor(config.Symbol);
        _levelCache = new LevelCache(config.CacheDir);

        // Wire up events
        _trader.OnTradeAction += async (_, action) =>
        {
            OnTradeAction?.Invoke(this, action);
            await _executor.ExecuteAsync(action);

            if (action is TradeAction.Exit exit)
            {
                if (exit.PnlPoints > 0) _wins++;
                else _losses++;
            }
            else if (action is TradeAction.Enter)
            {
                _signalCount++;
            }
        };

        _executor.OnLog += (_, msg) => Log(msg);
    }

    /// <summary>
    /// Initialize and start the trading strategy
    /// </summary>
    /// <param name="contractSymbol">Contract symbol (e.g., "MNQH6")</param>
    public async Task StartAsync(string contractSymbol)
    {
        if (_isRunning)
            throw new InvalidOperationException("Strategy already running");

        Log("═══════════════════════════════════════════════════════════");
        Log("           LVN TRADING STRATEGY - STARTING                 ");
        Log("═══════════════════════════════════════════════════════════");
        Log($"Symbol: {contractSymbol}");
        Log($"Contracts: {_config.Contracts}");
        Log($"Trading Hours: {_config.StartHour:00}:{_config.StartMinute:00} - {_config.EndHour:00}:{_config.EndMinute:00} ET");
        Log("");

        // Load cached daily levels
        var cachedLevels = _levelCache.LoadLevels();
        if (cachedLevels != null && _levelCache.AreLevelsFresh(cachedLevels))
        {
            Log($"Loaded cached levels for {cachedLevels.Date}:");
            Log($"  PDH: {cachedLevels.Pdh:F2}  PDL: {cachedLevels.Pdl:F2}");
            Log($"  VAH: {cachedLevels.Vah:F2}  VAL: {cachedLevels.Val:F2}");
            _trader.SetDailyLevels(cachedLevels);
        }
        else
        {
            Log("Warning: No fresh daily levels cached. Will need to compute from historical data.");
        }

        // Initialize executor
        _executor.Initialize();

        // Initialize data client
        _dataClient = DatabentoClient.FromEnvironment(contractSymbol);
        _barAggregator = new BarAggregator(Symbols.GetBaseSymbol(contractSymbol));

        _cts = new CancellationTokenSource();
        _isRunning = true;

        // Start trading loop
        _tradingTask = RunTradingLoopAsync(contractSymbol, _cts.Token);

        Log("");
        Log("═══════════════════════════════════════════════════════════");
        Log("                     TRADING ACTIVE                         ");
        Log("═══════════════════════════════════════════════════════════");
    }

    /// <summary>
    /// Stop the trading strategy
    /// </summary>
    public async Task StopAsync()
    {
        if (!_isRunning) return;

        Log("Stopping strategy...");

        _cts?.Cancel();

        if (_tradingTask != null)
        {
            try { await _tradingTask; }
            catch (OperationCanceledException) { }
        }

        // Flatten any open position
        if (_trader.HasPosition)
        {
            await _executor.ExecuteAsync(new TradeAction.FlattenAll("Strategy stopped"));
        }

        await (_dataClient?.StopAsync() ?? Task.CompletedTask);

        _isRunning = false;

        Log("");
        Log("═══════════════════════════════════════════════════════════");
        Log("                    SESSION COMPLETE                        ");
        Log("═══════════════════════════════════════════════════════════");
        Log($"Total Signals: {_signalCount}");
        Log($"Wins: {_wins} | Losses: {_losses}");
        Log($"P&L: {_trader.DailyPnl:F2} pts");
    }

    private async Task RunTradingLoopAsync(string contractSymbol, CancellationToken ct)
    {
        if (_dataClient == null || _barAggregator == null)
            throw new InvalidOperationException("Data client not initialized");

        try
        {
            await _dataClient.StartAsync();
            Log("Connected to Databento, streaming trades...");

            await foreach (var trade in _dataClient.GetTradesAsync().WithCancellation(ct))
            {
                // Feed trade to impulse profiler if active
                if (_trader.IsProfilingImpulse)
                {
                    _trader.ProcessTrade(trade);
                }

                // Aggregate to bars
                var isBuy = trade.Side == Side.Buy;
                var bar = _barAggregator.ProcessTrade(
                    trade.TsEvent,
                    trade.Price,
                    trade.Size,
                    isBuy
                );

                if (bar != null)
                {
                    // Process bar through trader
                    var action = _trader.ProcessBar(bar);

                    // Action is already executed via event handler
                }
            }
        }
        catch (OperationCanceledException)
        {
            // Expected when stopping
        }
        catch (Exception ex)
        {
            Log($"Error in trading loop: {ex.Message}");
            throw;
        }
    }

    /// <summary>
    /// Get current trading statistics
    /// </summary>
    public TradingStats GetStats() => _trader.GetStats();

    /// <summary>
    /// Check if strategy is running
    /// </summary>
    public bool IsRunning => _isRunning;

    /// <summary>
    /// Check if in position
    /// </summary>
    public bool HasPosition => _trader.HasPosition;

    private void Log(string message)
    {
        var timestamped = $"[{DateTime.Now:HH:mm:ss}] {message}";
        OnLog?.Invoke(this, timestamped);
        Console.WriteLine(timestamped);
    }

    public async ValueTask DisposeAsync()
    {
        await StopAsync();
        _cts?.Dispose();
    }
}

/// <summary>
/// Example usage for Optimus Flow integration
/// </summary>
/*
// In your Optimus Flow Strategy class:

public class MyLvnStrategy : Strategy
{
    private LvnTradingStrategy? _strategy;

    protected override void OnRun()
    {
        var config = TradingConfig.DefaultMnq();
        _strategy = new LvnTradingStrategy(config);
        _strategy.OnLog += (_, msg) => Log(msg);

        // Start async (fire and forget in Quantower context)
        Task.Run(() => _strategy.StartAsync("MNQH6"));
    }

    protected override void OnStop()
    {
        _strategy?.StopAsync().GetAwaiter().GetResult();
    }

    protected override string[] OnGetMetrics()
    {
        var stats = _strategy?.GetStats();
        return new[]
        {
            $"State: {stats?.CurrentState}",
            $"P&L: {stats?.DailyPnl:F2} pts",
            $"Losses: {stats?.DailyLosses}"
        };
    }
}
*/
