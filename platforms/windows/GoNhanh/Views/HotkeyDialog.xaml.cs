using System.Windows;
using System.Windows.Input;
using GoNhanh.Core;
using WpfApplication = System.Windows.Application;

namespace GoNhanh.Views;

/// <summary>
/// Dialog for recording a new keyboard shortcut
/// </summary>
public partial class HotkeyDialog : Window
{
    private KeyboardShortcut? _recordedShortcut;

    /// <summary>
    /// The recorded shortcut (null if cancelled)
    /// </summary>
    public KeyboardShortcut? RecordedShortcut => _recordedShortcut;

    public HotkeyDialog()
    {
        InitializeComponent();
    }

    private void Window_Loaded(object sender, RoutedEventArgs e)
    {
        // Disable keyboard hook while dialog is open
        ((GoNhanh.App)WpfApplication.Current).SetKeyboardHookEnabled(false);

        // Ensure window has focus
        Activate();
        Focus();
    }

    private void Window_KeyDown(object sender, System.Windows.Input.KeyEventArgs e)
    {
        var key = e.Key == Key.System ? e.SystemKey : e.Key;

        // Ignore modifier-only keys
        if (key == Key.LeftCtrl || key == Key.RightCtrl ||
            key == Key.LeftAlt || key == Key.RightAlt ||
            key == Key.LeftShift || key == Key.RightShift ||
            key == Key.LWin || key == Key.RWin)
        {
            return;
        }

        // ESC closes dialog
        if (key == Key.Escape)
        {
            Cancel_Click(sender, e);
            return;
        }

        // Get modifiers
        bool ctrl = Keyboard.Modifiers.HasFlag(ModifierKeys.Control);
        bool alt = Keyboard.Modifiers.HasFlag(ModifierKeys.Alt);
        bool shift = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);

        // Require at least one modifier
        if (!ctrl && !alt && !shift)
        {
            ShortcutDisplay.Text = "Cần có Ctrl/Alt/Shift!";
            OkButton.IsEnabled = false;
            e.Handled = true;
            return;
        }

        // Get virtual key code
        int vk = KeyInterop.VirtualKeyFromKey(key);

        // Validate - block system shortcuts
        if (IsSystemShortcut(ctrl, alt, shift, vk))
        {
            ShortcutDisplay.Text = "Phím tắt hệ thống!";
            OkButton.IsEnabled = false;
            e.Handled = true;
            return;
        }

        // Build modifiers byte
        byte modifiers = 0;
        if (ctrl) modifiers |= KeyboardShortcut.MOD_CTRL;
        if (alt) modifiers |= KeyboardShortcut.MOD_ALT;
        if (shift) modifiers |= KeyboardShortcut.MOD_SHIFT;

        // Create shortcut
        _recordedShortcut = new KeyboardShortcut
        {
            KeyCode = (ushort)vk,
            Modifiers = modifiers
        };

        // Update display
        ShortcutDisplay.Text = _recordedShortcut.ToDisplayString();
        OkButton.IsEnabled = true;

        e.Handled = true;
    }

    private void Ok_Click(object sender, RoutedEventArgs e)
    {
        // Re-enable keyboard hook
        ((GoNhanh.App)WpfApplication.Current).SetKeyboardHookEnabled(true);
        DialogResult = true;
        Close();
    }

    private void Cancel_Click(object sender, RoutedEventArgs e)
    {
        // Re-enable keyboard hook
        ((GoNhanh.App)WpfApplication.Current).SetKeyboardHookEnabled(true);
        _recordedShortcut = null;
        DialogResult = false;
        Close();
    }

    protected override void OnClosed(EventArgs e)
    {
        // Ensure keyboard hook is re-enabled
        ((GoNhanh.App)WpfApplication.Current).SetKeyboardHookEnabled(true);
        base.OnClosed(e);
    }

    private static bool IsSystemShortcut(bool ctrl, bool alt, bool shift, int vk)
    {
        // Block Ctrl+C, Ctrl+V, Ctrl+X, Ctrl+A, Ctrl+Z, Ctrl+Y
        if (ctrl && !alt && !shift)
        {
            return vk is 0x43 or 0x56 or 0x58 or 0x41 or 0x5A or 0x59; // C, V, X, A, Z, Y
        }

        // Block Alt+Tab, Alt+F4
        if (alt && !ctrl && !shift)
        {
            return vk is 0x09 or 0x73; // Tab, F4
        }

        return false;
    }
}
