using Microsoft.Win32;
using System;
using System.Collections.Generic;
using System.Linq;
using GoNhanh.Core;

namespace GoNhanh.Services;

/// <summary>
/// Manages user shortcuts (abbreviations like vn → Việt Nam)
/// Persists shortcuts to Registry
/// </summary>
public class ShortcutsManager
{
    private const string RegistryPath = @"Software\GoNhanh\Shortcuts";
    private readonly Dictionary<string, string> _shortcuts = new();

    /// <summary>
    /// Get all shortcuts as dictionary
    /// </summary>
    public IReadOnlyDictionary<string, string> Shortcuts => _shortcuts;

    /// <summary>
    /// Load shortcuts from Registry and sync with Rust engine
    /// </summary>
    public void Load()
    {
        _shortcuts.Clear();
        RustBridge.ClearShortcuts();

        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RegistryPath);
            if (key == null) return;

            foreach (var valueName in key.GetValueNames())
            {
                var replacement = key.GetValue(valueName) as string;
                if (!string.IsNullOrEmpty(replacement))
                {
                    _shortcuts[valueName] = replacement;
                    RustBridge.AddShortcut(valueName, replacement);
                }
            }

            System.Diagnostics.Debug.WriteLine($"[Shortcuts] Loaded {_shortcuts.Count} shortcuts");
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[Shortcuts] Load error: {ex.Message}");
        }
    }

    /// <summary>
    /// Save all shortcuts to Registry
    /// </summary>
    public void Save()
    {
        try
        {
            // Delete old key and recreate to ensure clean state
            Registry.CurrentUser.DeleteSubKeyTree(RegistryPath, throwOnMissingSubKey: false);

            using var key = Registry.CurrentUser.CreateSubKey(RegistryPath);
            if (key == null) return;

            foreach (var (trigger, replacement) in _shortcuts)
            {
                key.SetValue(trigger, replacement, RegistryValueKind.String);
            }

            System.Diagnostics.Debug.WriteLine($"[Shortcuts] Saved {_shortcuts.Count} shortcuts");
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[Shortcuts] Save error: {ex.Message}");
        }
    }

    /// <summary>
    /// Add or update a shortcut
    /// </summary>
    public void Add(string trigger, string replacement)
    {
        if (string.IsNullOrWhiteSpace(trigger) || string.IsNullOrWhiteSpace(replacement))
            return;

        _shortcuts[trigger] = replacement;
        RustBridge.AddShortcut(trigger, replacement);
        Save();
    }

    /// <summary>
    /// Remove a shortcut
    /// </summary>
    public void Remove(string trigger)
    {
        if (_shortcuts.Remove(trigger))
        {
            RustBridge.RemoveShortcut(trigger);
            Save();
        }
    }

    /// <summary>
    /// Clear all shortcuts
    /// </summary>
    public void Clear()
    {
        _shortcuts.Clear();
        RustBridge.ClearShortcuts();
        Save();
    }

    /// <summary>
    /// Add default shortcuts (Vietnamese common abbreviations)
    /// </summary>
    public void LoadDefaults()
    {
        var defaults = new Dictionary<string, string>
        {
            { "vn", "Việt Nam" },
            { "hn", "Hà Nội" },
            { "hcm", "Hồ Chí Minh" },
            { "tphcm", "Thành phố Hồ Chí Minh" },
            { "ko", "không" },
            { "kg", "không" },
            { "dc", "được" },
            { "k", "không" },
            { "vs", "với" },
            { "ms", "mới" },
        };

        foreach (var (trigger, replacement) in defaults)
        {
            if (!_shortcuts.ContainsKey(trigger))
            {
                _shortcuts[trigger] = replacement;
                RustBridge.AddShortcut(trigger, replacement);
            }
        }

        Save();
    }
}
