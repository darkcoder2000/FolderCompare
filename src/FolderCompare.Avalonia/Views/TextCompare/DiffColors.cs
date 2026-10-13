using Avalonia;
using Avalonia.Media;
using Avalonia.Media.Immutable;

namespace FolderCompare.App.Views.TextCompare;

/// <summary>Brushes of the text compare view; defined in Themes/Colors.axaml.</summary>
internal static class DiffColors
{
    public static IBrush ChangedLine => Get("TextDiffChangedLineBrush");
    public static IBrush ChangedText => Get("TextDiffChangedTextBrush");
    public static IBrush UnimportantLine => Get("TextDiffUnimportantLineBrush");
    public static IBrush UnimportantText => Get("TextDiffUnimportantTextBrush");
    public static IBrush Filler => Get("TextDiffFillerBrush");
    public static IBrush FillerHatch => Get("TextDiffFillerHatchBrush");
    public static IBrush CurrentSection => Get("TextDiffCurrentSectionBrush");
    public static IBrush Glyph => Get("GlyphBrush");
    public static IBrush CenterStrip => Get("TextDiffCenterStripBrush");
    public static IBrush Overview => Get("TextDiffOverviewBrush");
    public static IBrush ViewportBorder => Get("TextDiffViewportBorderBrush");
    public static IBrush ViewportFill => Get("TextDiffViewportFillBrush");

    private static IBrush Get(string key)
    {
        var app = Application.Current;
        if (app is not null && app.TryGetResource(key, app.ActualThemeVariant, out var value) && value is IBrush brush)
            return brush is ISolidColorBrush solid ? new ImmutableSolidColorBrush(solid.Color, solid.Opacity) : brush;
        return Brushes.Magenta; // a missing key in Colors.axaml should be obvious
    }
}
