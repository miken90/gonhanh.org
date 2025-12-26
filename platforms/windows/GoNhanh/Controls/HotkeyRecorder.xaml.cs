using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using GoNhanh.Core;
using WpfColor = System.Windows.Media.Color;
using WpfMessageBox = System.Windows.MessageBox;

namespace GoNhanh.Controls;

/// <summary>
/// UserControl for recording keyboard shortcuts
/// Matches macOS ShortcutRecorderRow pattern
/// </summary>
public partial class HotkeyRecorder : System.Windows.Controls.UserControl
{
    private bool _isRecording;
    private KeyboardShortcut _shortcut = KeyboardShortcut.Default;

    /// <summary>
    /// Event raised when shortcut changes
    /// </summary>
    public event EventHandler<KeyboardShortcut>? ShortcutChanged;

    public HotkeyRecorder()
    {
        InitializeComponent();
        UpdateDisplay();
    }

    /// <summary>
    /// Get or set the current shortcut
    /// </summary>
    public KeyboardShortcut Shortcut
    {
        get => _shortcut;
        set
        {
            _shortcut = value;
            UpdateDisplay();
        }
    }

    private void Border_MouseDown(object sender, MouseButtonEventArgs e)
    {
        StartRecording();
    }

    private void StartRecording()
    {
        _isRecording = true;
        MainBorder.BorderBrush = new SolidColorBrush(WpfColor.FromRgb(37, 99, 235)); // Primary blue
        KeysPanel.Visibility = Visibility.Collapsed;
        RecordingText.Visibility = Visibility.Visible;
        Focus();
    }

    private void StopRecording()
    {
        _isRecording = false;
        MainBorder.BorderBrush = new SolidColorBrush(WpfColor.FromRgb(229, 231, 235)); // #E5E7EB
        KeysPanel.Visibility = Visibility.Visible;
        RecordingText.Visibility = Visibility.Collapsed;
    }

    protected override void OnKeyDown(System.Windows.Input.KeyEventArgs e)
    {
        if (!_isRecording) return;

        // Ignore modifier-only keys
        if (e.Key == Key.LeftCtrl || e.Key == Key.RightCtrl ||
            e.Key == Key.LeftAlt || e.Key == Key.RightAlt ||
            e.Key == Key.LeftShift || e.Key == Key.RightShift ||
            e.Key == Key.System)
        {
            return;
        }

        // Get modifiers
        bool ctrl = Keyboard.Modifiers.HasFlag(ModifierKeys.Control);
        bool alt = Keyboard.Modifiers.HasFlag(ModifierKeys.Alt);
        bool shift = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift);

        // Require at least one modifier
        if (!ctrl && !alt && !shift)
        {
            // ESC cancels recording
            if (e.Key == Key.Escape)
            {
                StopRecording();
                e.Handled = true;
                return;
            }
            return;
        }

        // Get virtual key code
        int vk = KeyInterop.VirtualKeyFromKey(e.Key == Key.System ? e.SystemKey : e.Key);

        // Validate - block system shortcuts
        if (IsSystemShortcut(ctrl, alt, shift, vk))
        {
            WpfMessageBox.Show(
                "Phím tắt này đã được hệ thống sử dụng.",
                "Không thể sử dụng",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
            e.Handled = true;
            return;
        }

        // Build modifiers byte
        byte modifiers = 0;
        if (ctrl) modifiers |= KeyboardShortcut.MOD_CTRL;
        if (alt) modifiers |= KeyboardShortcut.MOD_ALT;
        if (shift) modifiers |= KeyboardShortcut.MOD_SHIFT;

        // Create new shortcut
        _shortcut = new KeyboardShortcut
        {
            KeyCode = (ushort)vk,
            Modifiers = modifiers
        };

        StopRecording();
        UpdateDisplay();
        ShortcutChanged?.Invoke(this, _shortcut);

        e.Handled = true;
    }

    protected override void OnLostFocus(RoutedEventArgs e)
    {
        base.OnLostFocus(e);
        if (_isRecording)
        {
            StopRecording();
        }
    }

    private void ClearButton_Click(object sender, RoutedEventArgs e)
    {
        _shortcut = KeyboardShortcut.Default;
        UpdateDisplay();
        ShortcutChanged?.Invoke(this, _shortcut);
    }

    private void UpdateDisplay()
    {
        KeysPanel.Children.Clear();
        var parts = _shortcut.GetDisplayParts();

        for (int i = 0; i < parts.Length; i++)
        {
            if (i > 0)
            {
                KeysPanel.Children.Add(new TextBlock
                {
                    Text = "+",
                    Margin = new Thickness(4, 0, 4, 0),
                    VerticalAlignment = VerticalAlignment.Center,
                    Foreground = new SolidColorBrush(WpfColor.FromRgb(156, 163, 175))
                });
            }

            // Keycap style border
            var keycap = new Border
            {
                Background = new SolidColorBrush(WpfColor.FromRgb(243, 244, 246)), // #F3F4F6
                BorderBrush = new SolidColorBrush(WpfColor.FromRgb(209, 213, 219)), // #D1D5DB
                BorderThickness = new Thickness(1),
                CornerRadius = new CornerRadius(4),
                Padding = new Thickness(8, 4, 8, 4),
                Child = new TextBlock
                {
                    Text = parts[i],
                    FontSize = 12,
                    FontWeight = FontWeights.Medium,
                    Foreground = new SolidColorBrush(WpfColor.FromRgb(55, 65, 81)) // #374151
                }
            };

            KeysPanel.Children.Add(keycap);
        }
    }

    /// <summary>
    /// Check if shortcut conflicts with common system shortcuts
    /// </summary>
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
