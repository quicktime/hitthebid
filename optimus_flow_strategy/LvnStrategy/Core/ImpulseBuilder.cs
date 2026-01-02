using LvnStrategy.Models;

namespace LvnStrategy.Core;

/// <summary>
/// Tracks an impulse leg in real-time and calculates its score.
/// Score based on 5 criteria: broke_swing, fast, uniform, volume_increased, sufficient_size
/// Matches Rust implementation from src/trading_core/impulse.rs
/// </summary>
public class ImpulseBuilder
{
    private readonly ImpulseDirection _direction;
    private readonly List<Bar> _bars = new();

    private double _startPrice;
    private double _extremePrice;
    private double _retracePrice;
    private ulong _totalVolume;
    private long _totalDelta;
    private double _swingHigh;
    private double _swingLow;

    public ImpulseBuilder(ImpulseDirection direction, Bar firstBar)
    {
        _direction = direction;
        _startPrice = firstBar.Open;
        _extremePrice = direction == ImpulseDirection.Up ? firstBar.High : firstBar.Low;
        _retracePrice = _extremePrice;
        _swingHigh = firstBar.High;
        _swingLow = firstBar.Low;
        AddBar(firstBar);
    }

    /// <summary>
    /// Default constructor for initialization
    /// </summary>
    public ImpulseBuilder()
    {
        _direction = ImpulseDirection.Up;
        _startPrice = 0;
        _extremePrice = 0;
        _retracePrice = 0;
    }

    /// <summary>
    /// Add a bar to the impulse
    /// </summary>
    public void AddBar(Bar bar)
    {
        _bars.Add(bar);
        _totalVolume += bar.Volume;
        _totalDelta += bar.Delta;

        // Update swing points
        _swingHigh = Math.Max(_swingHigh, bar.High);
        _swingLow = Math.Min(_swingLow, bar.Low);

        if (_direction == ImpulseDirection.Up)
        {
            // Track extreme high and retrace low
            if (bar.High > _extremePrice)
            {
                _extremePrice = bar.High;
                _retracePrice = bar.Low; // Reset retrace at new high
            }
            _retracePrice = Math.Min(_retracePrice, bar.Low);
        }
        else
        {
            // Track extreme low and retrace high
            if (bar.Low < _extremePrice)
            {
                _extremePrice = bar.Low;
                _retracePrice = bar.High; // Reset retrace at new low
            }
            _retracePrice = Math.Max(_retracePrice, bar.High);
        }
    }

    /// <summary>
    /// Get the total impulse size in points
    /// </summary>
    public double GetImpulseSize()
    {
        return Math.Abs(_extremePrice - _startPrice);
    }

    /// <summary>
    /// Get the retrace ratio (how much of the impulse has been retraced)
    /// </summary>
    public double GetRetraceRatio()
    {
        var impulseSize = GetImpulseSize();
        if (impulseSize < 0.01) return 0;

        var retraceSize = _direction == ImpulseDirection.Up
            ? _extremePrice - _retracePrice
            : _retracePrice - _extremePrice;

        return retraceSize / impulseSize;
    }

    /// <summary>
    /// Calculate impulse score (0-5)
    /// Criteria: broke_swing, fast, uniform, volume_increased, sufficient_size
    /// </summary>
    public int CalculateScore()
    {
        var score = 0;

        // 1. Broke Swing: Did the impulse break prior swing high/low?
        var brokeSwing = _direction == ImpulseDirection.Up
            ? _extremePrice > _swingHigh
            : _extremePrice < _swingLow;
        if (brokeSwing) score++;

        // 2. Fast: Completed in reasonable time (< 60 bars = 1 min)
        if (_bars.Count < 60) score++;

        // 3. Uniform: Consistent direction (cumulative delta in direction)
        var deltaInDirection = _direction == ImpulseDirection.Up
            ? _totalDelta > 0
            : _totalDelta < 0;
        if (deltaInDirection) score++;

        // 4. Volume Increased: Later bars have more volume than early bars
        if (_bars.Count >= 4)
        {
            var firstHalfVolume = _bars.Take(_bars.Count / 2).Sum(b => (long)b.Volume);
            var secondHalfVolume = _bars.Skip(_bars.Count / 2).Sum(b => (long)b.Volume);
            if (secondHalfVolume > firstHalfVolume) score++;
        }
        else
        {
            score++; // Give benefit of doubt for short impulses
        }

        // 5. Sufficient Size: At least 10 points
        if (GetImpulseSize() >= 10.0) score++;

        return score;
    }

    /// <summary>
    /// Get all bars in this impulse
    /// </summary>
    public IReadOnlyList<Bar> GetBars() => _bars.AsReadOnly();

    /// <summary>
    /// Get the impulse direction
    /// </summary>
    public ImpulseDirection Direction => _direction;

    /// <summary>
    /// Get the extreme price reached
    /// </summary>
    public double ExtremePrice => _extremePrice;

    /// <summary>
    /// Get the start price
    /// </summary>
    public double StartPrice => _startPrice;

    /// <summary>
    /// Get cumulative delta
    /// </summary>
    public long TotalDelta => _totalDelta;
}
