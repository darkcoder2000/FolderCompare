using System.Windows;
using System.Windows.Media;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>Brushes of the text compare view; defined in Themes/Colors.xaml, with fallbacks.</summary>
internal static class DiffColors
{
    public static Brush ChangedLine => Get("TextDiffChangedLineBrush", "#FFE6E6");
    public static Brush ChangedText => Get("TextDiffChangedTextBrush", "#FFAAAA");
    public static Brush UnimportantLine => Get("TextDiffUnimportantLineBrush", "#E6EEFF");
    public static Brush UnimportantText => Get("TextDiffUnimportantTextBrush", "#B8CFFF");
    public static Brush Filler => Get("TextDiffFillerBrush", "#F2F2F2");
    public static Brush FillerHatch => Get("TextDiffFillerHatchBrush", "#DDDDDD");
    public static Brush CurrentSection => Get("TextDiffCurrentSectionBrush", "#E07B00");
    public static Brush Glyph => Get("GlyphBrush", "#444444");

    private static readonly Dictionary<string, Brush> Fallbacks = new();

    private static Brush Get(string key, string fallback)
    {
        if (Application.Current?.TryFindResource(key) is Brush b) return b;
        if (!Fallbacks.TryGetValue(key, out var brush))
        {
            brush = new SolidColorBrush((Color)ColorConverter.ConvertFromString(fallback));
            brush.Freeze();
            Fallbacks[key] = brush;
        }
        return brush;
    }
}
