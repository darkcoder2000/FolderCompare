using System.Diagnostics;
using TimeDiff.Core.Models;

namespace TimeDiff.Core.Comparison;

/// <summary>Thread-safe scan progress counter that reports at most every 100 ms.</summary>
internal sealed class ProgressTracker
{
    private static readonly long IntervalTicks = Stopwatch.Frequency / 10;

    private readonly IProgress<ScanProgress>? _progress;
    private readonly object _lock = new();
    private long _items;
    private long _lastReport;
    private volatile string _currentFolder = "";

    public ProgressTracker(IProgress<ScanProgress>? progress) => _progress = progress;

    public long Items => Interlocked.Read(ref _items);

    public void Item()
    {
        Interlocked.Increment(ref _items);
        MaybeReport();
    }

    public void Folder(string path)
    {
        _currentFolder = path;
        MaybeReport();
    }

    public void ReportFinal() => _progress?.Report(new ScanProgress(Items, ""));

    private void MaybeReport()
    {
        if (_progress is null) return;
        long now = Stopwatch.GetTimestamp();
        if (now - Interlocked.Read(ref _lastReport) < IntervalTicks) return;
        lock (_lock)
        {
            if (now - _lastReport < IntervalTicks) return;
            Interlocked.Exchange(ref _lastReport, now);
        }
        _progress.Report(new ScanProgress(Items, _currentFolder));
    }
}
