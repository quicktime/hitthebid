namespace LvnStrategy.Models;

/// <summary>
/// Base class for all trade actions.
/// Matches Rust enum TradeAction from src/trading_core/trader.rs
/// </summary>
public abstract class TradeAction
{
    /// <summary>Enter a new position</summary>
    public class Enter : TradeAction
    {
        public Direction Direction { get; init; }
        public double Price { get; init; }
        public double Stop { get; init; }
        public double Target { get; init; }
        public int Contracts { get; init; }

        public Enter(Direction direction, double price, double stop, double target, int contracts)
        {
            Direction = direction;
            Price = price;
            Stop = stop;
            Target = target;
            Contracts = contracts;
        }
    }

    /// <summary>Exit current position</summary>
    public class Exit : TradeAction
    {
        public Direction Direction { get; init; }
        public double Price { get; init; }
        public double PnlPoints { get; init; }
        public string Reason { get; init; }

        public Exit(Direction direction, double price, double pnlPoints, string reason)
        {
            Direction = direction;
            Price = price;
            PnlPoints = pnlPoints;
            Reason = reason;
        }
    }

    /// <summary>Update stop loss price</summary>
    public class UpdateStop : TradeAction
    {
        public double NewStop { get; init; }

        public UpdateStop(double newStop)
        {
            NewStop = newStop;
        }
    }

    /// <summary>Signal pending for next bar (no action needed)</summary>
    public class SignalPending : TradeAction { }

    /// <summary>Flatten all positions immediately</summary>
    public class FlattenAll : TradeAction
    {
        public string Reason { get; init; }

        public FlattenAll(string reason)
        {
            Reason = reason;
        }
    }
}
