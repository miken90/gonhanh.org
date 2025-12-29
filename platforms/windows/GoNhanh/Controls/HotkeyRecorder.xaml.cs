using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using GoNhanh.Core;
using GoNhanh.Views;
using WpfColor = System.Windows.Media.Color;

namespace GoNhanh.Controls;

/// <summary>
/// UserControl for recording keyboard shortcuts
/// Opens dialog for clear UX
/// </summary>
public partial class HotkeyRecorder : System.Windows.Controls.UserControl
{
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
        // Ignore if click originated from ClearButton
        var source = e.OriginalSource as DependencyObject;
        while (source != null && source != this)
        {
            if (source == ClearButton) return;
            source = VisualTreeHelper.GetParent(source);
        }

        // Open dialog to record new hotkey
        var dialog = new HotkeyDialog();
        dialog.Owner = Window.GetWindow(this);

        if (dialog.ShowDialog() == true && dialog.RecordedShortcut != null)
        {
            _shortcut = dialog.RecordedShortcut;
            UpdateDisplay();
            ShortcutChanged?.Invoke(this, _shortcut);
        }
    }

    private void ClearButton_PreviewMouseDown(object sender, MouseButtonEventArgs e)
    {
        // Clear shortcut
        _shortcut = KeyboardShortcut.Empty;
        UpdateDisplay();
        ShortcutChanged?.Invoke(this, _shortcut);
        e.Handled = true;
    }

    private void ClearButton_Click(object sender, RoutedEventArgs e)
    {
        e.Handled = true;
    }

    private void UpdateDisplay()
    {
        KeysPanel.Children.Clear();

        // Show placeholder if empty
        if (_shortcut.IsEmpty)
        {
            KeysPanel.Children.Add(new TextBlock
            {
                Text = "Nhấp để đặt phím tắt...",
                Foreground = new SolidColorBrush(WpfColor.FromRgb(156, 163, 175)), // #9CA3AF
                FontStyle = FontStyles.Italic,
                VerticalAlignment = VerticalAlignment.Center
            });
            return;
        }

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
}
