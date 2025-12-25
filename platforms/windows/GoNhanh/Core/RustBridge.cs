using System.Runtime.InteropServices;
using System.Text;

namespace GoNhanh.Core;

/// <summary>
/// P/Invoke bridge to Rust core library (gonhanh_core.dll)
/// FFI contract matches core/src/lib.rs exports
/// </summary>
public static class RustBridge
{
    private const string DllName = "gonhanh_core.dll";

    #region Native Imports

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_init();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_clear();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_free(IntPtr result);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_method(byte method);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_enabled([MarshalAs(UnmanagedType.U1)] bool enabled);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_modern([MarshalAs(UnmanagedType.U1)] bool modern);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ime_key_ext(
        ushort keycode,
        [MarshalAs(UnmanagedType.U1)] bool capslock,
        [MarshalAs(UnmanagedType.U1)] bool ctrl,
        [MarshalAs(UnmanagedType.U1)] bool shift);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_skip_w_shortcut([MarshalAs(UnmanagedType.U1)] bool skip);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_esc_restore([MarshalAs(UnmanagedType.U1)] bool enabled);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_free_tone([MarshalAs(UnmanagedType.U1)] bool enabled);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_english_auto_restore([MarshalAs(UnmanagedType.U1)] bool enabled);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_auto_capitalize([MarshalAs(UnmanagedType.U1)] bool enabled);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_clear_all();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int ime_get_buffer(IntPtr outPtr, int maxLen);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern void ime_restore_word(IntPtr wordPtr);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern void ime_add_shortcut(IntPtr triggerPtr, IntPtr replacementPtr);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    private static extern void ime_remove_shortcut(IntPtr triggerPtr);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void ime_clear_shortcuts();

    #endregion

    #region Public API

    /// <summary>
    /// Initialize the IME engine. Call once at startup.
    /// </summary>
    public static void Initialize()
    {
        ime_init();
    }

    /// <summary>
    /// Clear the typing buffer.
    /// </summary>
    public static void Clear()
    {
        ime_clear();
    }

    /// <summary>
    /// Set input method (Telex=0, VNI=1)
    /// </summary>
    public static void SetMethod(InputMethod method)
    {
        ime_method((byte)method);
    }

    /// <summary>
    /// Enable or disable IME processing
    /// </summary>
    public static void SetEnabled(bool enabled)
    {
        ime_enabled(enabled);
    }

    /// <summary>
    /// Set tone style (modern=true: hòa, old=false: hoà)
    /// </summary>
    public static void SetModernTone(bool modern)
    {
        ime_modern(modern);
    }

    /// <summary>
    /// Process a keystroke and get the result
    /// Translates Windows Virtual Key codes to macOS keycodes for the Rust core
    /// </summary>
    /// <param name="keycode">Windows Virtual Key code</param>
    /// <param name="shift">Shift key pressed</param>
    /// <param name="capslock">CapsLock is on</param>
    public static ImeResult ProcessKey(ushort keycode, bool shift, bool capslock)
    {
        // Translate Windows VK to macOS keycode (Rust core uses macOS keycodes)
        ushort macKeycode = TranslateToMacKeycode(keycode);
        if (macKeycode == 0xFFFF) // Unknown key
        {
            return ImeResult.Empty;
        }

        try
        {
            // Call ime_key_ext with correct parameter order: (key, caps, ctrl, shift)
            IntPtr ptr = ime_key_ext(macKeycode, capslock, false, shift);

            if (ptr == IntPtr.Zero)
            {
                return ImeResult.Empty;
            }

            try
            {
                var native = Marshal.PtrToStructure<NativeResult>(ptr);
                return ImeResult.FromNative(native);
            }
            finally
            {
                ime_free(ptr);
            }
        }
        catch
        {
            return ImeResult.Empty;
        }
    }

    /// <summary>
    /// Debug version of ProcessKey that returns pointer status
    /// </summary>
    public static (ImeResult result, bool ptrWasNull) ProcessKeyDebug(ushort keycode, bool shift, bool capslock)
    {
        ushort macKeycode = TranslateToMacKeycode(keycode);
        if (macKeycode == 0xFFFF)
        {
            return (ImeResult.Empty, true);
        }

        try
        {
            IntPtr ptr = ime_key_ext(macKeycode, capslock, false, shift);

            if (ptr == IntPtr.Zero)
            {
                return (ImeResult.Empty, true); // NULL pointer = engine not initialized
            }

            try
            {
                var native = Marshal.PtrToStructure<NativeResult>(ptr);
                return (ImeResult.FromNative(native), false);
            }
            finally
            {
                ime_free(ptr);
            }
        }
        catch
        {
            return (ImeResult.Empty, true);
        }
    }

    /// <summary>
    /// Set whether to skip w→ư shortcut in Telex mode
    /// When skip is true, typing 'w' at word start stays as 'w' instead of converting to 'ư'
    /// </summary>
    public static void SetSkipWShortcut(bool skip)
    {
        ime_skip_w_shortcut(skip);
    }

    /// <summary>
    /// Set whether ESC key restores raw ASCII input
    /// When enabled (default), pressing ESC restores original keystrokes
    /// When disabled, ESC key is passed through without restoration
    /// </summary>
    public static void SetEscRestore(bool enabled)
    {
        ime_esc_restore(enabled);
    }

    /// <summary>
    /// Set whether to enable free tone placement (skip validation)
    /// When enabled, allows placing diacritics anywhere without spelling validation
    /// When disabled (default), validates Vietnamese spelling rules
    /// </summary>
    public static void SetFreeTone(bool enabled)
    {
        ime_free_tone(enabled);
    }

    /// <summary>
    /// Set whether to enable English auto-restore (experimental feature)
    /// When enabled, automatically restores English words that were accidentally transformed
    /// When disabled (default), no auto-restore happens
    /// </summary>
    public static void SetEnglishAutoRestore(bool enabled)
    {
        ime_english_auto_restore(enabled);
    }

    /// <summary>
    /// Set whether to enable auto-capitalize after sentence-ending punctuation
    /// When enabled, automatically capitalizes first letter after . ! ? Enter
    /// When disabled (default), no auto-capitalize happens
    /// </summary>
    public static void SetAutoCapitalize(bool enabled)
    {
        ime_auto_capitalize(enabled);
    }

    /// <summary>
    /// Clear buffer and word history
    /// Call when cursor position changes (mouse click, arrow keys, focus change)
    /// This prevents accidental restore from stale history
    /// </summary>
    public static void ClearAll()
    {
        ime_clear_all();
    }

    /// <summary>
    /// Get the full composed buffer as UTF-32 codepoints
    /// Used for "Select All + Replace" injection method where the entire buffer content is needed
    /// </summary>
    /// <param name="maxLength">Maximum number of codepoints to retrieve (default 64)</param>
    /// <returns>The full buffer string</returns>
    public static string GetBuffer(int maxLength = 64)
    {
        IntPtr bufferPtr = Marshal.AllocHGlobal(maxLength * sizeof(uint));
        try
        {
            int length = ime_get_buffer(bufferPtr, maxLength);
            if (length <= 0)
                return string.Empty;

            var buffer = new uint[length];
            Marshal.Copy(bufferPtr, (int[])(object)buffer, 0, length);

            var sb = new StringBuilder(length);
            for (int i = 0; i < length; i++)
            {
                if (buffer[i] > 0)
                {
                    sb.Append(char.ConvertFromUtf32((int)buffer[i]));
                }
            }
            return sb.ToString();
        }
        finally
        {
            Marshal.FreeHGlobal(bufferPtr);
        }
    }

    /// <summary>
    /// Restore buffer from a Vietnamese word string
    /// Used when native app detects cursor at word boundary and user wants to continue editing
    /// Parses Vietnamese characters back to buffer components
    /// </summary>
    /// <param name="word">The Vietnamese word to restore</param>
    public static void RestoreWord(string word)
    {
        if (string.IsNullOrEmpty(word))
            return;

        byte[] bytes = Encoding.UTF8.GetBytes(word + '\0');
        IntPtr wordPtr = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, wordPtr, bytes.Length);
            ime_restore_word(wordPtr);
        }
        finally
        {
            Marshal.FreeHGlobal(wordPtr);
        }
    }

    /// <summary>
    /// Add a shortcut to the engine
    /// Auto-detects shortcut type: symbol triggers (like "->") use immediate trigger,
    /// alphabetic triggers (like "vn") use word boundary trigger
    /// </summary>
    /// <param name="trigger">Trigger string (e.g., "vn" or "->")</param>
    /// <param name="replacement">Replacement string (e.g., "Việt Nam")</param>
    public static void AddShortcut(string trigger, string replacement)
    {
        if (string.IsNullOrEmpty(trigger) || string.IsNullOrEmpty(replacement))
            return;

        byte[] triggerBytes = Encoding.UTF8.GetBytes(trigger + '\0');
        byte[] replacementBytes = Encoding.UTF8.GetBytes(replacement + '\0');

        IntPtr triggerPtr = Marshal.AllocHGlobal(triggerBytes.Length);
        IntPtr replacementPtr = Marshal.AllocHGlobal(replacementBytes.Length);
        try
        {
            Marshal.Copy(triggerBytes, 0, triggerPtr, triggerBytes.Length);
            Marshal.Copy(replacementBytes, 0, replacementPtr, replacementBytes.Length);
            ime_add_shortcut(triggerPtr, replacementPtr);
        }
        finally
        {
            Marshal.FreeHGlobal(triggerPtr);
            Marshal.FreeHGlobal(replacementPtr);
        }
    }

    /// <summary>
    /// Remove a shortcut from the engine
    /// </summary>
    /// <param name="trigger">Trigger string to remove</param>
    public static void RemoveShortcut(string trigger)
    {
        if (string.IsNullOrEmpty(trigger))
            return;

        byte[] bytes = Encoding.UTF8.GetBytes(trigger + '\0');
        IntPtr triggerPtr = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, triggerPtr, bytes.Length);
            ime_remove_shortcut(triggerPtr);
        }
        finally
        {
            Marshal.FreeHGlobal(triggerPtr);
        }
    }

    /// <summary>
    /// Clear all shortcuts from the engine
    /// </summary>
    public static void ClearShortcuts()
    {
        ime_clear_shortcuts();
    }

    #endregion

    #region Keycode Translation (Windows VK -> macOS)

    // macOS virtual keycodes (from core/src/data/keys.rs)
    private const ushort MAC_A = 0x00;
    private const ushort MAC_S = 0x01;
    private const ushort MAC_D = 0x02;
    private const ushort MAC_F = 0x03;
    private const ushort MAC_H = 0x04;
    private const ushort MAC_G = 0x05;
    private const ushort MAC_Z = 0x06;
    private const ushort MAC_X = 0x07;
    private const ushort MAC_C = 0x08;
    private const ushort MAC_V = 0x09;
    private const ushort MAC_B = 0x0B;
    private const ushort MAC_Q = 0x0C;
    private const ushort MAC_W = 0x0D;
    private const ushort MAC_E = 0x0E;
    private const ushort MAC_R = 0x0F;
    private const ushort MAC_Y = 0x10;
    private const ushort MAC_T = 0x11;
    private const ushort MAC_O = 0x1F;
    private const ushort MAC_U = 0x20;
    private const ushort MAC_I = 0x22;
    private const ushort MAC_P = 0x23;
    private const ushort MAC_L = 0x25;
    private const ushort MAC_J = 0x26;
    private const ushort MAC_K = 0x28;
    private const ushort MAC_N = 0x2D;
    private const ushort MAC_M = 0x2E;
    private const ushort MAC_N1 = 0x12;
    private const ushort MAC_N2 = 0x13;
    private const ushort MAC_N3 = 0x14;
    private const ushort MAC_N4 = 0x15;
    private const ushort MAC_N5 = 0x17;
    private const ushort MAC_N6 = 0x16;
    private const ushort MAC_N7 = 0x1A;
    private const ushort MAC_N8 = 0x1C;
    private const ushort MAC_N9 = 0x19;
    private const ushort MAC_N0 = 0x1D;
    private const ushort MAC_SPACE = 0x31;
    private const ushort MAC_DELETE = 0x33;
    private const ushort MAC_TAB = 0x30;
    private const ushort MAC_RETURN = 0x24;
    private const ushort MAC_ESC = 0x35;
    private const ushort MAC_LBRACKET = 0x21;
    private const ushort MAC_RBRACKET = 0x1E;

    /// <summary>
    /// Translate Windows Virtual Key code to macOS keycode
    /// Returns 0xFFFF if key is not mapped
    /// </summary>
    private static ushort TranslateToMacKeycode(ushort windowsVK)
    {
        return windowsVK switch
        {
            // Letters A-Z
            0x41 => MAC_A,  // VK_A
            0x42 => MAC_B,  // VK_B
            0x43 => MAC_C,  // VK_C
            0x44 => MAC_D,  // VK_D
            0x45 => MAC_E,  // VK_E
            0x46 => MAC_F,  // VK_F
            0x47 => MAC_G,  // VK_G
            0x48 => MAC_H,  // VK_H
            0x49 => MAC_I,  // VK_I
            0x4A => MAC_J,  // VK_J
            0x4B => MAC_K,  // VK_K
            0x4C => MAC_L,  // VK_L
            0x4D => MAC_M,  // VK_M
            0x4E => MAC_N,  // VK_N
            0x4F => MAC_O,  // VK_O
            0x50 => MAC_P,  // VK_P
            0x51 => MAC_Q,  // VK_Q
            0x52 => MAC_R,  // VK_R
            0x53 => MAC_S,  // VK_S
            0x54 => MAC_T,  // VK_T
            0x55 => MAC_U,  // VK_U
            0x56 => MAC_V,  // VK_V
            0x57 => MAC_W,  // VK_W
            0x58 => MAC_X,  // VK_X
            0x59 => MAC_Y,  // VK_Y
            0x5A => MAC_Z,  // VK_Z

            // Numbers 0-9
            0x30 => MAC_N0,  // VK_0
            0x31 => MAC_N1,  // VK_1
            0x32 => MAC_N2,  // VK_2
            0x33 => MAC_N3,  // VK_3
            0x34 => MAC_N4,  // VK_4
            0x35 => MAC_N5,  // VK_5
            0x36 => MAC_N6,  // VK_6
            0x37 => MAC_N7,  // VK_7
            0x38 => MAC_N8,  // VK_8
            0x39 => MAC_N9,  // VK_9

            // Special keys
            0x08 => MAC_DELETE,   // VK_BACK (Backspace)
            0x09 => MAC_TAB,      // VK_TAB
            0x0D => MAC_RETURN,   // VK_RETURN (Enter)
            0x1B => MAC_ESC,      // VK_ESCAPE
            0x20 => MAC_SPACE,    // VK_SPACE
            0xDB => MAC_LBRACKET, // VK_OEM_4 ([{)
            0xDD => MAC_RBRACKET, // VK_OEM_6 (]})

            _ => 0xFFFF  // Unknown key
        };
    }

    #endregion
}

/// <summary>
/// Input method type
/// </summary>
public enum InputMethod : byte
{
    Telex = 0,
    VNI = 1
}

/// <summary>
/// IME action type
/// </summary>
public enum ImeAction : byte
{
    None = 0,    // No action needed
    Send = 1,    // Send text replacement
    Restore = 2  // Restore original text
}

/// <summary>
/// Native result structure from Rust (must match core/src/engine/mod.rs)
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeResult
{
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 64)]  // MAX = 64 in Rust
    public uint[] chars;
    public byte action;
    public byte backspace;
    public byte count;
    public byte flags;  // Was _pad, now correctly named flags
}

/// <summary>
/// Managed IME result
/// </summary>
public readonly struct ImeResult
{
    public readonly ImeAction Action;
    public readonly byte Backspace;
    public readonly byte Count;
    private readonly uint[] _chars;

    public static readonly ImeResult Empty = new(ImeAction.None, 0, 0, Array.Empty<uint>());

    private ImeResult(ImeAction action, byte backspace, byte count, uint[] chars)
    {
        Action = action;
        Backspace = backspace;
        Count = count;
        _chars = chars;
    }

    internal static ImeResult FromNative(NativeResult native)
    {
        return new ImeResult(
            (ImeAction)native.action,
            native.backspace,
            native.count,
            native.chars ?? Array.Empty<uint>()
        );
    }

    /// <summary>
    /// Get the result text as a string
    /// </summary>
    public string GetText()
    {
        if (Count == 0 || _chars == null)
            return string.Empty;

        var sb = new StringBuilder(Count);
        for (int i = 0; i < Count && i < _chars.Length; i++)
        {
            if (_chars[i] > 0)
            {
                sb.Append(char.ConvertFromUtf32((int)_chars[i]));
            }
        }
        return sb.ToString();
    }
}
