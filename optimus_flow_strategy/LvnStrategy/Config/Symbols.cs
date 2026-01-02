namespace LvnStrategy.Config;

/// <summary>
/// Contract symbol mappings and specifications
/// </summary>
public static class Symbols
{
    /// <summary>
    /// Get the Databento symbol for a given contract
    /// </summary>
    public static string GetDatabentoSymbol(string contractSymbol)
    {
        // Databento uses raw symbols like "MNQH6" for March 2026 Micro NQ
        return contractSymbol;
    }

    /// <summary>
    /// Get the base symbol from a contract symbol (e.g., "MNQ" from "MNQH6")
    /// </summary>
    public static string GetBaseSymbol(string contractSymbol)
    {
        if (contractSymbol.StartsWith("MNQ")) return "MNQ";
        if (contractSymbol.StartsWith("NQ")) return "NQ";
        if (contractSymbol.StartsWith("MES")) return "MES";
        if (contractSymbol.StartsWith("ES")) return "ES";
        if (contractSymbol.StartsWith("MCL")) return "MCL";
        if (contractSymbol.StartsWith("CL")) return "CL";

        // Fallback: strip last 2 characters (month+year)
        return contractSymbol.Length > 2
            ? contractSymbol[..^2]
            : contractSymbol;
    }

    /// <summary>
    /// Get the point value for a symbol
    /// </summary>
    public static double GetPointValue(string baseSymbol)
    {
        return baseSymbol switch
        {
            "MNQ" => 0.50,   // Micro NQ: $0.50 per 0.25 tick = $2 per point
            "NQ" => 5.00,    // E-mini NQ: $5 per 0.25 tick = $20 per point
            "MES" => 0.50,   // Micro ES: $0.50 per 0.25 tick = $1.25 per point
            "ES" => 12.50,   // E-mini ES: $12.50 per 0.25 tick = $50 per point
            "MCL" => 1.00,   // Micro CL: $1 per 0.01 tick
            "CL" => 10.00,   // Crude Oil: $10 per 0.01 tick
            _ => 1.00        // Default
        };
    }

    /// <summary>
    /// Get tick size for a symbol
    /// </summary>
    public static double GetTickSize(string baseSymbol)
    {
        return baseSymbol switch
        {
            "MNQ" or "NQ" => 0.25,
            "MES" or "ES" => 0.25,
            "MCL" or "CL" => 0.01,
            _ => 0.01
        };
    }

    /// <summary>
    /// Month codes for futures contracts
    /// </summary>
    public static class MonthCodes
    {
        public const char January = 'F';
        public const char February = 'G';
        public const char March = 'H';
        public const char April = 'J';
        public const char May = 'K';
        public const char June = 'M';
        public const char July = 'N';
        public const char August = 'Q';
        public const char September = 'U';
        public const char October = 'V';
        public const char November = 'X';
        public const char December = 'Z';

        public static char GetCode(int month)
        {
            return month switch
            {
                1 => January,
                2 => February,
                3 => March,
                4 => April,
                5 => May,
                6 => June,
                7 => July,
                8 => August,
                9 => September,
                10 => October,
                11 => November,
                12 => December,
                _ => throw new ArgumentException($"Invalid month: {month}")
            };
        }
    }
}
