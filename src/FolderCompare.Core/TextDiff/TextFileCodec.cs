using System.IO.Abstractions;
using System.Text;

namespace FolderCompare.Core.TextDiff;

/// <summary>A text file as loaded for comparison, with what is needed to write it back unchanged.</summary>
public sealed record LoadedText(string Text, Encoding Encoding, bool HasBom, string NewLine, bool IsBinary)
{
    public static LoadedText Empty { get; } = new("", new UTF8Encoding(false), false, "\r\n", false);

    public string EncodingName => Encoding.CodePage switch
    {
        65001 => HasBom ? "UTF-8 BOM" : "UTF-8",
        1200 => "UTF-16 LE",
        1201 => "UTF-16 BE",
        1252 => "ANSI",
        _ => Encoding.WebName,
    };

    public string NewLineName => NewLine switch
    {
        "\r\n" => "CRLF",
        "\n" => "LF",
        _ => "CR",
    };
}

public static class TextFileCodec
{
    private const int BinaryProbeLength = 8000;

    static TextFileCodec() => Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);

    public static LoadedText Load(IFileSystem fs, string path) => Decode(fs.File.ReadAllBytes(path));

    public static LoadedText Decode(byte[] bytes)
    {
        Encoding encoding;
        int bomLength = 0;
        if (bytes.Length >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF)
        {
            encoding = new UTF8Encoding(false);
            bomLength = 3;
        }
        else if (bytes.Length >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE)
        {
            encoding = new UnicodeEncoding(bigEndian: false, byteOrderMark: false);
            bomLength = 2;
        }
        else if (bytes.Length >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF)
        {
            encoding = new UnicodeEncoding(bigEndian: true, byteOrderMark: false);
            bomLength = 2;
        }
        else
        {
            if (Array.IndexOf(bytes, (byte)0, 0, Math.Min(bytes.Length, BinaryProbeLength)) >= 0)
                return new LoadedText("", new UTF8Encoding(false), false, "\r\n", IsBinary: true);
            encoding = IsValidUtf8(bytes) ? new UTF8Encoding(false) : Encoding.GetEncoding(1252);
        }

        var text = encoding.GetString(bytes, bomLength, bytes.Length - bomLength);
        return new LoadedText(text, encoding, bomLength > 0, DetectNewLine(text), false);
    }

    /// <summary>Writes text with the given encoding, adding the byte order mark when the original had one.</summary>
    public static void Save(IFileSystem fs, string path, string text, LoadedText format)
    {
        var preamble = format.HasBom ? BomFor(format.Encoding) : Array.Empty<byte>();
        var body = format.Encoding.GetBytes(text);
        var bytes = new byte[preamble.Length + body.Length];
        preamble.CopyTo(bytes, 0);
        body.CopyTo(bytes, preamble.Length);
        fs.File.WriteAllBytes(path, bytes);
    }

    private static byte[] BomFor(Encoding encoding) => encoding.CodePage switch
    {
        65001 => new byte[] { 0xEF, 0xBB, 0xBF },
        1200 => new byte[] { 0xFF, 0xFE },
        1201 => new byte[] { 0xFE, 0xFF },
        _ => Array.Empty<byte>(),
    };

    private static bool IsValidUtf8(byte[] bytes)
    {
        try
        {
            new UTF8Encoding(false, throwOnInvalidBytes: true).GetCharCount(bytes);
            return true;
        }
        catch (DecoderFallbackException)
        {
            return false;
        }
    }

    /// <summary>The most frequent line ending; CRLF when the text has none.</summary>
    public static string DetectNewLine(string text)
    {
        int crlf = 0, lf = 0, cr = 0;
        for (int n = 0; n < text.Length; n++)
        {
            if (text[n] == '\r')
            {
                if (n + 1 < text.Length && text[n + 1] == '\n') { crlf++; n++; }
                else cr++;
            }
            else if (text[n] == '\n')
            {
                lf++;
            }
        }
        if (lf > crlf && lf >= cr) return "\n";
        if (cr > crlf && cr > lf) return "\r";
        return "\r\n";
    }
}
