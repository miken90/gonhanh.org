using System.Diagnostics;

namespace GoNhanh.Core;

/// <summary>
/// Background thread processor for keyboard events.
/// Consumes events from KeyEventQueue, processes via RustBridge, and sends output.
/// Thread.Sleep() in worker doesn't block the hook callback.
/// </summary>
public sealed class KeyboardWorker : IDisposable
{
    private readonly KeyEventQueue _queue;
    private readonly Thread _workerThread;
    private volatile bool _running;
    private int _disposed;  // 0 = active, 1 = disposed (Interlocked for thread-safe)

    /// <summary>
    /// Callback invoked for each key event. Set by App.xaml.cs.
    /// </summary>
    public Action<KeyEvent>? OnKeyProcess { get; set; }

    public KeyboardWorker(KeyEventQueue queue)
    {
        _queue = queue ?? throw new ArgumentNullException(nameof(queue));
        _workerThread = new Thread(ProcessLoop)
        {
            Name = "KeyboardWorker",
            IsBackground = true,
            Priority = ThreadPriority.AboveNormal  // Higher priority for responsiveness
        };
    }

    /// <summary>
    /// Start the worker thread. Call after setting OnKeyProcess.
    /// </summary>
    public void Start()
    {
        if (_running) return;
        _running = true;
        _workerThread.Start();
    }

    /// <summary>
    /// Stop the worker thread gracefully.
    /// </summary>
    /// <param name="timeoutMs">Max wait time for thread to exit</param>
    public void Stop(int timeoutMs = 1000)
    {
        if (!_running) return;
        _running = false;

        // Wait for thread to finish
        if (_workerThread.IsAlive && !_workerThread.Join(timeoutMs))
        {
            // Thread didn't exit in time - log warning
            Debug.WriteLine($"KeyboardWorker: Thread did not exit within {timeoutMs}ms");
        }
    }

    /// <summary>
    /// Main processing loop - runs on dedicated thread.
    /// </summary>
    private void ProcessLoop()
    {
        Debug.WriteLine("KeyboardWorker: Started");

        while (_running)
        {
            try
            {
                // TryDequeue blocks until item available or timeout (1ms for low latency)
                if (_queue.TryDequeue(out var evt, timeoutMs: 1))
                {
                    ProcessKey(evt);
                }
            }
            catch (Exception ex)
            {
                // Log error but don't crash - continue processing
                Debug.WriteLine($"KeyboardWorker error: {ex.Message}");
            }
        }

        Debug.WriteLine("KeyboardWorker: Stopped");
    }

    /// <summary>
    /// Process a single key event.
    /// </summary>
    private void ProcessKey(KeyEvent evt)
    {
        // Delegate to handler (set by App.xaml.cs)
        // Handler runs RustBridge.ProcessKey + TextSender.SendText
        OnKeyProcess?.Invoke(evt);
    }

    public void Dispose()
    {
        // Atomic exchange - ensures single disposal
        if (Interlocked.Exchange(ref _disposed, 1) == 1) return;

        Stop();
        // Note: Don't dispose _queue here - App owns it and may dispose separately
    }
}
