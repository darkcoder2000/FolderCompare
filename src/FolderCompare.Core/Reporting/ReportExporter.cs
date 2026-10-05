using System.Globalization;
using FolderCompare.Core.Models;

namespace FolderCompare.Core.Reporting;

public static class ReportExporter
{
    private const string DateFormat = "yyyy-MM-dd HH:mm:ss";

    public static void WriteCsv(TextWriter writer, DiffResult result, IEnumerable<DiffNode> nodes)
    {
        writer.WriteLine("RelativePath,Type,Status,LeftSize,LeftModified,RightSize,RightModified,Message");
        foreach (var n in nodes)
        {
            writer.WriteLine(string.Join(",",
                Csv(n.RelativePath),
                n.IsDirectory ? "Folder" : "File",
                n.Status,
                n.Left is null ? "" : n.LeftTotalSize.ToString(CultureInfo.InvariantCulture),
                n.Left is null ? "" : FormatDate(n.Left.LastWriteUtc),
                n.Right is null ? "" : n.RightTotalSize.ToString(CultureInfo.InvariantCulture),
                n.Right is null ? "" : FormatDate(n.Right.LastWriteUtc),
                Csv(n.ErrorMessage ?? "")));
        }
    }

    public static void WriteText(TextWriter writer, DiffResult result, IEnumerable<DiffNode> nodes)
    {
        writer.WriteLine("FolderCompare comparison report");
        writer.WriteLine($"Left:  {result.LeftRoot}");
        writer.WriteLine($"Right: {result.RightRoot}");
        writer.WriteLine($"Created: {DateTime.Now.ToString(DateFormat, CultureInfo.InvariantCulture)}");
        writer.WriteLine();
        foreach (var n in nodes)
        {
            var line = $"{n.Status,-12} {n.RelativePath}{(n.IsDirectory ? "\\" : "")}";
            if (n.Left is not null) line += $"  L: {FormatDate(n.Left.LastWriteUtc)}";
            if (n.Right is not null) line += $"  R: {FormatDate(n.Right.LastWriteUtc)}";
            if (n.ErrorMessage is not null) line += $"  ({n.ErrorMessage})";
            writer.WriteLine(line);
        }
    }

    private static string FormatDate(DateTime utc) => utc.ToLocalTime().ToString(DateFormat, CultureInfo.InvariantCulture);

    private static string Csv(string value) =>
        value.IndexOfAny([',', '"', '\n', '\r']) >= 0 ? "\"" + value.Replace("\"", "\"\"") + "\"" : value;
}
