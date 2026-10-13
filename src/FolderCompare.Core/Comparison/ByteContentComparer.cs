using System.Buffers;
using System.IO.Abstractions;

namespace FolderCompare.Core.Comparison;

/// <summary>Streams both files in chunks and stops at the first difference. Opens files read-only.</summary>
public sealed class ByteContentComparer : IContentComparer
{
    public const int BufferSize = 128 * 1024;

    private readonly IFileSystem _fs;

    public ByteContentComparer(IFileSystem fs) => _fs = fs;

    public bool AreEqual(string leftPath, string rightPath, Action<long>? bytesRead, CancellationToken ct)
    {
        ct.ThrowIfCancellationRequested();
        using var left = Open(leftPath);
        using var right = Open(rightPath);
        if (left.Length != right.Length) return false;

        var a = ArrayPool<byte>.Shared.Rent(BufferSize);
        var b = ArrayPool<byte>.Shared.Rent(BufferSize);
        try
        {
            while (true)
            {
                ct.ThrowIfCancellationRequested();
                int na = Fill(left, a);
                int nb = Fill(right, b);
                if (na != nb || !a.AsSpan(0, na).SequenceEqual(b.AsSpan(0, nb))) return false;
                if (na == 0) return true;
                bytesRead?.Invoke(na);
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(a);
            ArrayPool<byte>.Shared.Return(b);
        }
    }

    private Stream Open(string path) =>
        _fs.FileStream.New(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete,
                           BufferSize, FileOptions.SequentialScan);

    /// <summary>Reads until the chunk is full or the stream ends, so short reads cannot misalign both sides.</summary>
    private static int Fill(Stream stream, byte[] buffer)
    {
        int total = 0;
        while (total < BufferSize)
        {
            int n = stream.Read(buffer, total, BufferSize - total);
            if (n == 0) break;
            total += n;
        }
        return total;
    }
}
