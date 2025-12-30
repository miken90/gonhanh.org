using System.Windows;
using GoNhanh.Core;
using GoNhanh.Services;
using GoNhanh.Views;

namespace GoNhanh;

/// <summary>
/// GoNhanh - Vietnamese Input Method for Windows
/// Main application entry point
/// Matches macOS App.swift flow
/// </summary>
public partial class App : System.Windows.Application
{
    private TrayIcon? _trayIcon;
    private KeyboardHook? _keyboardHook;
    private KeyEventQueue? _keyQueue;      // Phase 2: Async queue for keyboard events
    private KeyboardWorker? _keyWorker;    // Phase 2: Background processor
    private readonly SettingsService _settings = new();
    private readonly ShortcutsManager _shortcuts = new();
    private System.Threading.Mutex? _mutex;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        // Prevent multiple instances
        if (!EnsureSingleInstance())
        {
            Shutdown();
            return;
        }

        // Initialize Rust core engine
        RustBridge.Initialize();

        // Load settings
        _settings.Load();
        _shortcuts.Load();

        // If first run, add default shortcuts
        if (_settings.IsFirstRun)
        {
            _shortcuts.LoadDefaults();
        }

        ApplySettings();

        // Initialize async keyboard processing (Phase 2 + 3)
        _keyQueue = new KeyEventQueue();
        _keyWorker = new KeyboardWorker(_keyQueue);
        _keyWorker.OnKeyProcess = ProcessKeyFromWorker;

        // Initialize keyboard hook with async queue (Phase 3)
        _keyboardHook = new KeyboardHook();
        _keyboardHook.SetQueue(_keyQueue);  // Wire hook to queue
        _keyboardHook.Hotkey = _settings.ToggleHotkey;
        _keyboardHook.OnHotkeyTriggered += OnHotkeyTriggered;

        // Start worker before hook to ensure events are processed
        _keyWorker.Start();
        _keyboardHook.Start();

        // Initialize system tray
        _trayIcon = new TrayIcon();
        _trayIcon.OnExitRequested += ExitApplication;
        _trayIcon.OnMethodChanged += ChangeInputMethod;
        _trayIcon.OnEnabledChanged += ToggleEnabled;
        _trayIcon.OnAdvancedSettingsRequested += ShowAdvancedSettings;
        _trayIcon.Initialize(_settings.CurrentMethod, _settings.IsEnabled);

        // Show onboarding if first run (like macOS)
        if (_settings.IsFirstRun)
        {
            ShowOnboarding();
        }
    }

    private bool EnsureSingleInstance()
    {
        _mutex = new System.Threading.Mutex(true, "GoNhanh_SingleInstance", out bool createdNew);
        if (!createdNew)
        {
            System.Windows.MessageBox.Show(
                $"{AppMetadata.Name} đang chạy.\nKiểm tra khay hệ thống (system tray).",
                AppMetadata.Name,
                MessageBoxButton.OK,
                MessageBoxImage.Information);
            return false;
        }
        return true;
    }

    public void ApplySettings()
    {
        // Existing settings
        RustBridge.SetMethod(_settings.CurrentMethod);
        RustBridge.SetEnabled(_settings.IsEnabled);
        RustBridge.SetModernTone(_settings.UseModernTone);

        // New advanced settings
        RustBridge.SetSkipWShortcut(_settings.SkipWShortcut);
        RustBridge.SetEscRestore(_settings.EscRestore);
        RustBridge.SetFreeTone(_settings.FreeTone);
        RustBridge.SetEnglishAutoRestore(_settings.EnglishAutoRestore);
        RustBridge.SetAutoCapitalize(_settings.AutoCapitalize);

        // Update hotkey in keyboard hook
        if (_keyboardHook != null)
        {
            _keyboardHook.Hotkey = _settings.ToggleHotkey;
        }
    }

    /// <summary>
    /// DEPRECATED: Synchronous key processing (Phase 1-2 legacy).
    /// Now replaced by ProcessKeyFromWorker() in async flow.
    /// Kept for reference only.
    /// </summary>
    [Obsolete("Use ProcessKeyFromWorker() for async processing")]
    private void OnKeyPressed(object? sender, KeyPressedEventArgs e)
    {
        if (!_settings.IsEnabled) return;

        var result = RustBridge.ProcessKey(e.VirtualKeyCode, e.Shift, e.CapsLock);

        if (result.Action == ImeAction.Send && result.Count > 0)
        {
            e.Handled = true;
            TextSender.SendText(result.GetText(), result.Backspace);
        }
        else if (result.Action == ImeAction.Restore)
        {
            e.Handled = true;
            TextSender.SendText(result.GetText(), result.Backspace);
        }
    }

    /// <summary>
    /// Process key from worker thread (Phase 3 async flow).
    /// Runs asynchronously - no blocking in hook callback.
    /// Note: SendInput works from any thread - no UI thread requirement.
    /// </summary>
    private void ProcessKeyFromWorker(KeyEvent evt)
    {
        // Settings.IsEnabled is atomic bool read - safe for cross-thread access
        if (!_settings.IsEnabled) return;

        var result = RustBridge.ProcessKey(evt.VirtualKeyCode, evt.Shift, evt.CapsLock);

        if (result.Action == ImeAction.Send && result.Count > 0)
        {
            TextSender.SendText(result.GetText(), result.Backspace);
        }
        else if (result.Action == ImeAction.Restore)
        {
            TextSender.SendText(result.GetText(), result.Backspace);
        }
    }

    private void ShowOnboarding()
    {
        var onboarding = new OnboardingWindow(_settings);
        onboarding.ShowDialog();

        // Save settings after onboarding
        _settings.IsFirstRun = false;
        _settings.Save();

        ApplySettings();
        _trayIcon?.UpdateState(_settings.CurrentMethod, _settings.IsEnabled);
    }

    private void ChangeInputMethod(InputMethod method)
    {
        _settings.CurrentMethod = method;
        _settings.Save();
        RustBridge.SetMethod(method);
    }

    private void ToggleEnabled(bool enabled)
    {
        _settings.IsEnabled = enabled;
        _settings.Save();
        RustBridge.SetEnabled(enabled);
    }

    private void OnHotkeyTriggered()
    {
        Dispatcher.Invoke(() =>
        {
            _settings.IsEnabled = !_settings.IsEnabled;
            _settings.Save();
            RustBridge.SetEnabled(_settings.IsEnabled);
            _trayIcon?.UpdateState(_settings.CurrentMethod, _settings.IsEnabled);
        });
    }

    /// <summary>
    /// Enable/disable global hotkey (used when recording new hotkey)
    /// </summary>
    public void SetHotkeyEnabled(bool enabled)
    {
        if (_keyboardHook != null)
        {
            _keyboardHook.HotkeyEnabled = enabled;
        }
    }

    /// <summary>
    /// Enable/disable entire keyboard hook (used when recording new hotkey in settings)
    /// </summary>
    public void SetKeyboardHookEnabled(bool enabled)
    {
        if (_keyboardHook != null)
        {
            if (enabled)
            {
                _keyboardHook.Start();
            }
            else
            {
                _keyboardHook.Stop();
            }
        }
    }

    private void ShowAdvancedSettings()
    {
        var advancedWindow = new AdvancedSettingsWindow(_settings, _shortcuts);
        advancedWindow.ShowDialog();
    }

    private void ExitApplication()
    {
        // Stop worker first (before hook, to prevent new events)
        _keyWorker?.Dispose();
        _keyQueue?.Dispose();

        _keyboardHook?.Stop();
        _keyboardHook?.Dispose();
        _trayIcon?.Dispose();
        RustBridge.Clear();
        _mutex?.Dispose();
        Shutdown();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _keyWorker?.Dispose();
        _keyQueue?.Dispose();
        _keyboardHook?.Dispose();
        _trayIcon?.Dispose();
        _mutex?.Dispose();
        base.OnExit(e);
    }
}
