using System.Diagnostics;
using System.Runtime.InteropServices;

namespace GoNhanh.Core;

/// <summary>
/// Low-level Windows keyboard hook for system-wide key interception
/// Similar to CGEventTap on macOS
/// </summary>
public class KeyboardHook : IDisposable
{
    #region Win32 Constants

    private const int WH_KEYBOARD_LL = 13;
    private const int WM_KEYDOWN = 0x0100;
    private const int WM_SYSKEYDOWN = 0x0104;
    private const uint LLKHF_INJECTED = 0x10;

    #endregion

    #region Win32 Imports

    private delegate IntPtr LowLevelKeyboardProc(int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr SetWindowsHookEx(int idHook, LowLevelKeyboardProc lpfn, IntPtr hMod, uint dwThreadId);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnhookWindowsHookEx(IntPtr hhk);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr GetModuleHandle(string lpModuleName);

    [DllImport("user32.dll")]
    private static extern short GetKeyState(int nVirtKey);

    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int vKey);

    #endregion

    #region Structures

    [StructLayout(LayoutKind.Sequential)]
    private struct KBDLLHOOKSTRUCT
    {
        public uint vkCode;
        public uint scanCode;
        public uint flags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    #endregion

    #region Fields

    private LowLevelKeyboardProc? _proc;
    private IntPtr _hookId = IntPtr.Zero;
    private bool _disposed;

    // Async queue for keyboard events (Phase 3)
    private KeyEventQueue? _queue;

    // Identifier for our injected keys (to skip processing them)
    private static readonly IntPtr InjectedKeyMarker = new IntPtr(0x474E4820); // "GNH " in hex

    #endregion

    #region Events

    // Note: KeyPressed event kept for backward compatibility but deprecated
    [Obsolete("Use SetQueue() for async processing instead")]
    public event EventHandler<KeyPressedEventArgs>? KeyPressed;

    /// <summary>
    /// Event raised when the toggle hotkey is pressed
    /// </summary>
    public event Action? OnHotkeyTriggered;

    #endregion

    #region Properties

    /// <summary>
    /// The hotkey that triggers toggle
    /// </summary>
    public KeyboardShortcut? Hotkey { get; set; }

    /// <summary>
    /// Enable/disable hotkey detection (used when recording new hotkey)
    /// </summary>
    public bool HotkeyEnabled { get; set; } = true;

    #endregion

    #region Public Methods

    /// <summary>
    /// Set the async queue for keyboard events.
    /// When set, HookCallback will enqueue events instead of invoking KeyPressed.
    /// </summary>
    public void SetQueue(KeyEventQueue queue)
    {
        _queue = queue;
    }

    /// <summary>
    /// Start the keyboard hook
    /// </summary>
    public void Start()
    {
        if (_hookId != IntPtr.Zero) return;

        _proc = HookCallback;
        using var curProcess = Process.GetCurrentProcess();
        using var curModule = curProcess.MainModule!;

        _hookId = SetWindowsHookEx(
            WH_KEYBOARD_LL,
            _proc,
            GetModuleHandle(curModule.ModuleName!),
            0);

        if (_hookId == IntPtr.Zero)
        {
            int error = Marshal.GetLastWin32Error();
            throw new System.ComponentModel.Win32Exception(error, $"Failed to install keyboard hook. Error: {error}");
        }
    }

    /// <summary>
    /// Stop the keyboard hook
    /// </summary>
    public void Stop()
    {
        if (_hookId != IntPtr.Zero)
        {
            UnhookWindowsHookEx(_hookId);
            _hookId = IntPtr.Zero;
        }
    }

    #endregion

    #region Private Methods

    private IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
    {
        // Only process key down events
        if (nCode >= 0 && (wParam == (IntPtr)WM_KEYDOWN || wParam == (IntPtr)WM_SYSKEYDOWN))
        {
            var hookStruct = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);

            // Skip our own injected keys
            if (hookStruct.dwExtraInfo == InjectedKeyMarker)
            {
                return CallNextHookEx(_hookId, nCode, wParam, lParam);
            }

            // Skip injected keys from other sources
            if ((hookStruct.flags & LLKHF_INJECTED) != 0)
            {
                return CallNextHookEx(_hookId, nCode, wParam, lParam);
            }

            ushort keyCode = (ushort)hookStruct.vkCode;

            // Check modifier keys
            bool shift = IsKeyDown(KeyCodes.VK_SHIFT);
            bool ctrl = IsKeyDown(KeyCodes.VK_CONTROL);
            bool alt = IsKeyDown(KeyCodes.VK_MENU);

            // Check for toggle hotkey FIRST (before any other processing)
            if (HotkeyEnabled && Hotkey != null && Hotkey.Matches(keyCode, ctrl, alt, shift))
            {
                OnHotkeyTriggered?.Invoke();
                return (IntPtr)1; // Consume the key
            }

            // Skip if Ctrl or Alt is pressed (shortcuts)
            if (ctrl || alt)
            {
                // Clear buffer on Ctrl+key combinations
                if (ctrl)
                {
                    RustBridge.Clear();
                }
                return CallNextHookEx(_hookId, nCode, wParam, lParam);
            }

            // Handle buffer-clearing keys (TAB, ESC only)
            if (keyCode == KeyCodes.VK_TAB || keyCode == KeyCodes.VK_ESCAPE)
            {
                RustBridge.Clear();
                return CallNextHookEx(_hookId, nCode, wParam, lParam);
            }

            // Only process relevant keys for Vietnamese input
            if (KeyCodes.IsRelevantKey(keyCode))
            {
                bool capsLock = IsCapsLockOn();

                // ASYNC MODE: Enqueue and return immediately (<1ms)
                if (_queue != null)
                {
                    _queue.Enqueue(new KeyEvent(keyCode, shift, capsLock));
                    return (IntPtr)1; // Block original key - worker will handle output
                }

                // LEGACY MODE: Synchronous processing (deprecated)
                #pragma warning disable CS0618 // Obsolete warning
                var args = new KeyPressedEventArgs(keyCode, shift, capsLock);
                KeyPressed?.Invoke(this, args);
                if (args.Handled)
                {
                    return (IntPtr)1;
                }
                #pragma warning restore CS0618
            }
        }

        return CallNextHookEx(_hookId, nCode, wParam, lParam);
    }

    private static bool IsKeyDown(int vKey)
    {
        return (GetAsyncKeyState(vKey) & 0x8000) != 0;
    }

    private static bool IsCapsLockOn()
    {
        return (GetKeyState(KeyCodes.VK_CAPITAL) & 0x0001) != 0;
    }

    #endregion

    #region IDisposable

    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    protected virtual void Dispose(bool disposing)
    {
        if (!_disposed)
        {
            Stop();
            _disposed = true;
        }
    }

    ~KeyboardHook()
    {
        Dispose(false);
    }

    #endregion

    /// <summary>
    /// Get the marker used to identify injected keys from this application
    /// </summary>
    public static IntPtr GetInjectedKeyMarker() => InjectedKeyMarker;
}

/// <summary>
/// Event args for key press events
/// </summary>
public class KeyPressedEventArgs : EventArgs
{
    public ushort VirtualKeyCode { get; }
    public bool Shift { get; }
    public bool CapsLock { get; }
    public bool Handled { get; set; }

    public KeyPressedEventArgs(ushort vkCode, bool shift, bool capsLock)
    {
        VirtualKeyCode = vkCode;
        Shift = shift;
        CapsLock = capsLock;
        Handled = false;
    }
}
