namespace GoNhanh.Core;

/// <summary>
/// Keyboard shortcut model for global hotkey
/// Stores key + modifiers, matching macOS KeyboardShortcut pattern
/// </summary>
public class KeyboardShortcut
{
    /// <summary>
    /// Virtual key code (VK_SPACE, VK_A, etc.)
    /// </summary>
    public ushort KeyCode { get; set; }

    /// <summary>
    /// Modifier flags (Ctrl=1, Alt=2, Shift=4)
    /// </summary>
    public byte Modifiers { get; set; }

    // Modifier flag constants
    public const byte MOD_CTRL = 1;
    public const byte MOD_ALT = 2;
    public const byte MOD_SHIFT = 4;

    /// <summary>
    /// Default hotkey: Ctrl+Space
    /// </summary>
    public static KeyboardShortcut Default => new()
    {
        KeyCode = KeyCodes.VK_SPACE,
        Modifiers = MOD_CTRL
    };

    /// <summary>
    /// Check if key press matches this shortcut
    /// </summary>
    public bool Matches(ushort keyCode, bool ctrl, bool alt, bool shift)
    {
        if (keyCode != KeyCode) return false;

        bool wantCtrl = (Modifiers & MOD_CTRL) != 0;
        bool wantAlt = (Modifiers & MOD_ALT) != 0;
        bool wantShift = (Modifiers & MOD_SHIFT) != 0;

        return ctrl == wantCtrl && alt == wantAlt && shift == wantShift;
    }

    /// <summary>
    /// Get display parts for UI (e.g., ["Ctrl", "Space"])
    /// </summary>
    public string[] GetDisplayParts()
    {
        var parts = new System.Collections.Generic.List<string>();

        if ((Modifiers & MOD_CTRL) != 0) parts.Add("Ctrl");
        if ((Modifiers & MOD_ALT) != 0) parts.Add("Alt");
        if ((Modifiers & MOD_SHIFT) != 0) parts.Add("Shift");

        parts.Add(GetKeyName(KeyCode));
        return parts.ToArray();
    }

    /// <summary>
    /// Get display string (e.g., "Ctrl+Space")
    /// </summary>
    public string ToDisplayString() => string.Join("+", GetDisplayParts());

    /// <summary>
    /// Serialize to registry string (e.g., "32,1" for Ctrl+Space)
    /// </summary>
    public string ToRegistryString() => $"{KeyCode},{Modifiers}";

    /// <summary>
    /// Parse from registry string
    /// </summary>
    public static KeyboardShortcut? FromRegistryString(string? value)
    {
        if (string.IsNullOrWhiteSpace(value)) return null;

        var parts = value.Split(',');
        if (parts.Length != 2) return null;

        if (!ushort.TryParse(parts[0], out ushort keyCode)) return null;
        if (!byte.TryParse(parts[1], out byte modifiers)) return null;

        return new KeyboardShortcut { KeyCode = keyCode, Modifiers = modifiers };
    }

    /// <summary>
    /// Get human-readable key name
    /// </summary>
    private static string GetKeyName(ushort keyCode) => keyCode switch
    {
        KeyCodes.VK_SPACE => "Space",
        KeyCodes.VK_RETURN => "Enter",
        KeyCodes.VK_TAB => "Tab",
        KeyCodes.VK_ESCAPE => "Esc",
        KeyCodes.VK_BACK => "Backspace",
        >= KeyCodes.VK_A and <= KeyCodes.VK_Z => ((char)keyCode).ToString(),
        >= KeyCodes.VK_0 and <= KeyCodes.VK_9 => ((char)keyCode).ToString(),
        KeyCodes.VK_OEM_COMMA => ",",
        KeyCodes.VK_OEM_PERIOD => ".",
        KeyCodes.VK_OEM_1 => ";",
        KeyCodes.VK_OEM_2 => "/",
        KeyCodes.VK_OEM_3 => "`",
        KeyCodes.VK_OEM_4 => "[",
        KeyCodes.VK_OEM_5 => "\\",
        KeyCodes.VK_OEM_6 => "]",
        KeyCodes.VK_OEM_7 => "'",
        KeyCodes.VK_OEM_PLUS => "=",
        KeyCodes.VK_OEM_MINUS => "-",
        _ => $"Key{keyCode:X2}"
    };
}
