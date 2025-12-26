using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Windows;
using GoNhanh.Core;
using GoNhanh.Services;

namespace GoNhanh.Views;

/// <summary>
/// Advanced Settings Window
/// Allows configuration of advanced features and text shortcuts
/// </summary>
public partial class AdvancedSettingsWindow : Window
{
    private readonly SettingsService _settings;
    private readonly ShortcutsManager _shortcuts;
    private readonly ObservableCollection<ShortcutItem> _shortcutItems = new();

    public AdvancedSettingsWindow(SettingsService settings, ShortcutsManager shortcuts)
    {
        InitializeComponent();

        _settings = settings;
        _shortcuts = shortcuts;

        LoadSettings();
        LoadShortcuts();
    }

    #region Load/Save

    private void LoadSettings()
    {
        // Load advanced feature toggles
        SkipWShortcutCheckBox.IsChecked = _settings.SkipWShortcut;
        EscRestoreCheckBox.IsChecked = _settings.EscRestore;
        FreeToneCheckBox.IsChecked = _settings.FreeTone;
        EnglishAutoRestoreCheckBox.IsChecked = _settings.EnglishAutoRestore;
        AutoCapitalizeCheckBox.IsChecked = _settings.AutoCapitalize;
        AutoStartCheckBox.IsChecked = _settings.AutoStart;

        // Load toggle hotkey
        HotkeyRecorderControl.Shortcut = _settings.ToggleHotkey;
    }

    private void LoadShortcuts()
    {
        _shortcutItems.Clear();

        foreach (var (trigger, replacement) in _shortcuts.Shortcuts)
        {
            _shortcutItems.Add(new ShortcutItem
            {
                Trigger = trigger,
                Replacement = replacement
            });
        }

        ShortcutsDataGrid.ItemsSource = _shortcutItems;
    }

    private void SaveSettings()
    {
        // Save advanced feature toggles
        _settings.SkipWShortcut = SkipWShortcutCheckBox.IsChecked ?? false;
        _settings.EscRestore = EscRestoreCheckBox.IsChecked ?? true;
        _settings.FreeTone = FreeToneCheckBox.IsChecked ?? false;
        _settings.EnglishAutoRestore = EnglishAutoRestoreCheckBox.IsChecked ?? false;
        _settings.AutoCapitalize = AutoCapitalizeCheckBox.IsChecked ?? true;
        _settings.AutoStart = AutoStartCheckBox.IsChecked ?? false;

        // Save toggle hotkey
        _settings.ToggleHotkey = HotkeyRecorderControl.Shortcut;

        _settings.Save();
    }

    #endregion

    #region Event Handlers

    private void AddShortcut_Click(object sender, RoutedEventArgs e)
    {
        var trigger = TriggerTextBox.Text?.Trim();
        var replacement = ReplacementTextBox.Text?.Trim();

        // Validate input
        if (string.IsNullOrWhiteSpace(trigger))
        {
            System.Windows.MessageBox.Show(
                "Vui lòng nhập phím tắt.",
                "Thiếu thông tin",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
            return;
        }

        if (string.IsNullOrWhiteSpace(replacement))
        {
            System.Windows.MessageBox.Show(
                "Vui lòng nhập văn bản thay thế.",
                "Thiếu thông tin",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
            return;
        }

        // Check for duplicates
        if (_shortcutItems.Any(s => s.Trigger == trigger))
        {
            System.Windows.MessageBox.Show(
                $"Phím tắt '{trigger}' đã tồn tại.",
                "Phím tắt trùng lặp",
                MessageBoxButton.OK,
                MessageBoxImage.Warning);
            return;
        }

        // Add shortcut
        _shortcuts.Add(trigger, replacement);

        // Update UI
        _shortcutItems.Add(new ShortcutItem
        {
            Trigger = trigger,
            Replacement = replacement
        });

        // Clear input
        TriggerTextBox.Clear();
        ReplacementTextBox.Clear();
        TriggerTextBox.Focus();
    }

    private void RemoveShortcut_Click(object sender, RoutedEventArgs e)
    {
        var selectedItem = ShortcutsDataGrid.SelectedItem as ShortcutItem;
        if (selectedItem == null)
        {
            System.Windows.MessageBox.Show(
                "Vui lòng chọn phím tắt cần xóa.",
                "Chưa chọn phím tắt",
                MessageBoxButton.OK,
                MessageBoxImage.Information);
            return;
        }

        // Confirm deletion
        var result = System.Windows.MessageBox.Show(
            $"Xóa phím tắt '{selectedItem.Trigger}'?",
            "Xác nhận xóa",
            MessageBoxButton.YesNo,
            MessageBoxImage.Question);

        if (result == MessageBoxResult.Yes)
        {
            _shortcuts.Remove(selectedItem.Trigger);
            _shortcutItems.Remove(selectedItem);
        }
    }

    private void Save_Click(object sender, RoutedEventArgs e)
    {
        SaveSettings();

        // Apply settings to Rust engine
        ((GoNhanh.App)System.Windows.Application.Current).ApplySettings();

        Close();
    }

    private void Cancel_Click(object sender, RoutedEventArgs e)
    {
        Close();
    }

    #endregion
}

/// <summary>
/// Data model for DataGrid shortcut items
/// </summary>
public class ShortcutItem : INotifyPropertyChanged
{
    private string _trigger = "";
    private string _replacement = "";

    public string Trigger
    {
        get => _trigger;
        set
        {
            _trigger = value;
            OnPropertyChanged(nameof(Trigger));
        }
    }

    public string Replacement
    {
        get => _replacement;
        set
        {
            _replacement = value;
            OnPropertyChanged(nameof(Replacement));
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    protected virtual void OnPropertyChanged(string propertyName)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}
