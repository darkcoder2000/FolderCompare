using Avalonia;
using Avalonia.Controls;
using FolderCompare.App.Services;

namespace FolderCompare.App.Views;

/// <summary>
/// Saves and restores window bounds. Positions are stored in device-independent pixels (screen pixels divided by
/// the screen's scaling), the same unit the WPF version used, so both share settings.json.
/// </summary>
internal static class WindowPlacement
{
    public static void Restore(Window window, WindowSettings? w, double minWidth, double minHeight)
    {
        if (w is null || w.Width < minWidth || w.Height < minHeight) return;

        // Only restore when the saved rectangle is still on one of the screens.
        var screen = window.Screens.All.FirstOrDefault(s =>
        {
            var scale = s.Scaling;
            var r = new PixelRect((int)(w.Left * scale), (int)(w.Top * scale), (int)(w.Width * scale), (int)(w.Height * scale));
            return s.Bounds.Intersects(r);
        });
        if (screen is null) return;

        window.WindowStartupLocation = WindowStartupLocation.Manual;
        window.Position = new PixelPoint((int)(w.Left * screen.Scaling), (int)(w.Top * screen.Scaling));
        window.Width = w.Width;
        window.Height = w.Height;
        if (w.Maximized) window.WindowState = WindowState.Maximized;
    }

    /// <summary>The bounds to save. A maximized window keeps its last normal bounds and only records the state.</summary>
    public static WindowSettings Save(Window window, WindowSettings? previous)
    {
        bool maximized = window.WindowState == WindowState.Maximized;
        if (window.WindowState != WindowState.Normal && previous is not null)
        {
            previous.Maximized = maximized;
            return previous;
        }
        double scale = window.Screens.ScreenFromWindow(window)?.Scaling ?? 1;
        return new WindowSettings
        {
            Left = window.Position.X / scale,
            Top = window.Position.Y / scale,
            Width = window.Width,
            Height = window.Height,
            Maximized = maximized,
        };
    }

    /// <summary>Shrinks and moves the window so it fits on its screen's working area.</summary>
    public static void FitToWorkArea(Window window)
    {
        if (window.WindowState != WindowState.Normal || window.Screens.ScreenFromWindow(window) is not { } screen) return;
        var work = screen.WorkingArea;
        double scale = screen.Scaling;
        window.Width = Math.Min(window.Width, work.Width / scale);
        window.Height = Math.Min(window.Height, work.Height / scale);
        int w = (int)(window.Width * scale), h = (int)(window.Height * scale);
        var p = window.Position;
        window.Position = new PixelPoint(Math.Clamp(p.X, work.X, Math.Max(work.X, work.Right - w)),
                                         Math.Clamp(p.Y, work.Y, Math.Max(work.Y, work.Bottom - h)));
    }
}
