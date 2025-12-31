using System.Diagnostics;
using System.Runtime.InteropServices;

namespace GoNhanh.Core;

/// <summary>
/// Detects foreground app and determines best text injection method.
/// Caches process lookups by window handle for performance (<1ms overhead).
/// </summary>
public static class AppDetector
{
    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    /// <summary>
    /// Apps requiring slow injection (Electron, terminals).
    /// Slow mode adds delays between backspaces and text for reliability.
    /// </summary>
    private static readonly HashSet<string> SlowApps = new(StringComparer.OrdinalIgnoreCase)
    {
        // Electron apps
        "claude", "notion", "slack", "discord", "teams",
        "code", "vscode", "cursor", "obsidian", "figma",
        // Terminals
        "windowsterminal", "cmd", "powershell", "pwsh",
        "wezterm", "alacritty", "hyper", "mintty", "wave", "waveterm",
        // Browsers (use slow mode as safe default)
        "chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc"
    };

    // Cache to avoid repeated process lookups
    private static string? _cachedProcessName;
    private static IntPtr _cachedWindow;

    /// <summary>
    /// Get injection method for current foreground app.
    /// Uses cached result if same window handle.
    /// </summary>
    public static InjectionMethod GetMethod()
    {
        var hwnd = GetForegroundWindow();

        // Use cache if same window
        if (hwnd == _cachedWindow && _cachedProcessName != null)
        {
            return DetermineMethod(_cachedProcessName);
        }

        // Get process name
        GetWindowThreadProcessId(hwnd, out uint pid);
        try
        {
            using var process = Process.GetProcessById((int)pid);
            _cachedProcessName = process.ProcessName;
            _cachedWindow = hwnd;
            return DetermineMethod(_cachedProcessName);
        }
        catch
        {
            return InjectionMethod.Fast;
        }
    }

    private static InjectionMethod DetermineMethod(string processName)
    {
        if (SlowApps.Contains(processName))
            return InjectionMethod.Slow;

        return InjectionMethod.Fast;
    }

    /// <summary>
    /// Force refresh cache (call on window change events)
    /// </summary>
    public static void InvalidateCache()
    {
        _cachedProcessName = null;
        _cachedWindow = IntPtr.Zero;
    }

    /// <summary>
    /// Get current cached process name (for debugging)
    /// </summary>
    public static string? GetCurrentProcessName() => _cachedProcessName;
}

/// <summary>
/// Text injection method based on target app compatibility
/// </summary>
public enum InjectionMethod
{
    /// <summary>Default: backspace + Unicode text in single batch</summary>
    Fast,
    /// <summary>With delays for slow apps (Electron, terminals, browsers)</summary>
    Slow
}
