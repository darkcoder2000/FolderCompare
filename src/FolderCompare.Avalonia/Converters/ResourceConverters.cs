using System.Globalization;
using Avalonia;
using Avalonia.Data.Converters;
using Avalonia.Media;
using FolderCompare.App.ViewModels;

namespace FolderCompare.App.Converters;

/// <summary>Looks up brushes and geometries in Themes/*.axaml, so colors stay defined in one place.</summary>
internal static class Resources
{
    public static object Find(string key) =>
        Application.Current is { } app && app.TryGetResource(key, app.ActualThemeVariant, out var value) && value is not null
            ? value
            : AvaloniaProperty.UnsetValue;
}

/// <summary>Text color of a side cell, from <see cref="SideState"/>.</summary>
public sealed class SideStateBrushConverter : IValueConverter
{
    public static readonly SideStateBrushConverter Instance = new();

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture) => value switch
    {
        SideState.Newer => Resources.Find("NewerBrush"),
        SideState.Older => Resources.Find("OlderBrush"),
        SideState.Only => Resources.Find("OnlyBrush"),
        SideState.Error => Resources.Find("ErrorBrush"),
        SideState.Conflict => Resources.Find("ConflictBrush"),
        _ => AvaloniaProperty.UnsetValue,
    };

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

/// <summary>Newer and conflicting sides are bold.</summary>
public sealed class SideStateWeightConverter : IValueConverter
{
    public static readonly SideStateWeightConverter Instance = new();

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is SideState.Newer or SideState.Conflict ? FontWeight.Bold : FontWeight.Normal;

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

/// <summary>Icon geometry for a folder, file or error row.</summary>
public sealed class IconKindGeometryConverter : IValueConverter
{
    public static readonly IconKindGeometryConverter Instance = new();

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture) => value switch
    {
        IconKind.Folder => Resources.Find("IconFolder"),
        IconKind.File => Resources.Find("IconFile"),
        IconKind.Warning => Resources.Find("IconWarning"),
        _ => null,
    };

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class IconKindBrushConverter : IValueConverter
{
    public static readonly IconKindBrushConverter Instance = new();

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture) => value switch
    {
        IconKind.Folder => Resources.Find("FolderIconBrush"),
        IconKind.Warning => Resources.Find("ErrorBrush"),
        _ => Resources.Find("FileIconBrush"),
    };

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}
