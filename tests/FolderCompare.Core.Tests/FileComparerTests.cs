using System.IO.Abstractions.TestingHelpers;
using FolderCompare.Core.Comparison;
using FolderCompare.Core.Models;
using static FolderCompare.Core.Tests.TestFs;

namespace FolderCompare.Core.Tests;

public class FileComparerTests
{
    private static readonly CompareOptions Off = new();
    private static readonly CompareOptions WhenTimesEqual = new() { ContentCompare = ContentCompareMode.WhenTimestampsEqual };
    private static readonly CompareOptions WhenSizesEqual = new() { ContentCompare = ContentCompareMode.WhenSizesEqual };

    private readonly MockFileSystem _fs = Create();

    private FileInfoSnapshot File(string path, DateTime utc, string content) =>
        File(path, utc, System.Text.Encoding.UTF8.GetBytes(content));

    private FileInfoSnapshot File(string path, DateTime utc, byte[] content)
    {
        _fs.AddFile(path, new MockFileData(content));
        _fs.File.SetLastWriteTimeUtc(path, utc);
        return new FileInfoSnapshot(content.Length, utc, FileAttributes.Normal, path);
    }

    private DiffStatus Compare(FileInfoSnapshot l, FileInfoSnapshot r, CompareOptions o) =>
        FileComparer.Compare(l, r, o, new ByteContentComparer(_fs), CancellationToken.None).Status;

    [Theory]
    [InlineData(ContentCompareMode.Off)]
    [InlineData(ContentCompareMode.WhenTimestampsEqual)]
    [InlineData(ContentCompareMode.WhenSizesEqual)]
    public void SameTime_DifferentSize_IsSameTimeDifferentContent(ContentCompareMode mode)
    {
        var l = File(P($@"{L}\a.txt"), T0, "short");
        var r = File(P($@"{R}\a.txt"), T0, "a lot longer");
        Assert.Equal(DiffStatus.SameTimeDifferentContent, Compare(l, r, new CompareOptions { ContentCompare = mode }));
    }

    [Fact]
    public void SameTime_SameSize_DifferentContent_ModeOff_IsIdentical()
    {
        // Known limitation of the fast mode: the content is not read.
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0, "xyz");
        Assert.Equal(DiffStatus.Identical, Compare(l, r, Off));
    }

    [Fact]
    public void SameTime_SameSize_DifferentContent_Verified_IsSameTimeDifferentContent()
    {
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0.AddSeconds(1), "xyz");
        Assert.Equal(DiffStatus.SameTimeDifferentContent, Compare(l, r, WhenTimesEqual));
        Assert.Equal(DiffStatus.SameTimeDifferentContent, Compare(l, r, WhenSizesEqual));
    }

    [Fact]
    public void SameTime_SameContent_Verified_IsIdentical()
    {
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0, "abc");
        Assert.Equal(DiffStatus.Identical, Compare(l, r, WhenTimesEqual));
    }

    [Fact]
    public void DifferentTime_SameContent()
    {
        var l = File(P($@"{L}\a.txt"), T0.AddHours(3), "abc");
        var r = File(P($@"{R}\a.txt"), T0, "abc");
        Assert.Equal(DiffStatus.NewerLeft, Compare(l, r, Off));
        Assert.Equal(DiffStatus.NewerLeft, Compare(l, r, WhenTimesEqual));
        Assert.Equal(DiffStatus.SameContentDifferentTime, Compare(l, r, WhenSizesEqual));
    }

    [Fact]
    public void DifferentTime_SameSize_DifferentContent_KeepsDirection()
    {
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0.AddHours(3), "xyz");
        Assert.Equal(DiffStatus.NewerRight, Compare(l, r, WhenSizesEqual));
    }

    [Fact]
    public void DifferentTime_DifferentSize_KeepsDirection()
    {
        var l = File(P($@"{L}\a.txt"), T0.AddMinutes(5), "short");
        var r = File(P($@"{R}\a.txt"), T0, "a lot longer");
        Assert.Equal(DiffStatus.NewerLeft, Compare(l, r, WhenSizesEqual));
        Assert.Equal(DiffStatus.NewerRight, Compare(r, l, Off));
    }

    [Fact]
    public void DstOffset_WithDifferentContent_IsNotIdentical()
    {
        var options = WhenTimesEqual with { IgnoreDstOffset = true };
        var l = File(P($@"{L}\a.txt"), T0.AddHours(1), "abc");
        var r = File(P($@"{R}\a.txt"), T0, "xyz");
        Assert.Equal(DiffStatus.SameTimeDifferentContent, Compare(l, r, options));
    }

    [Fact]
    public void LargeFile_DifferingOnlyInLastByte_IsDetected()
    {
        var a = new byte[ByteContentComparer.BufferSize * 3 + 17];
        new Random(42).NextBytes(a);
        var b = (byte[])a.Clone();
        b[^1] ^= 0xFF;
        var l = File(P($@"{L}\big.bin"), T0, a);
        var r = File(P($@"{R}\big.bin"), T0, b);
        var same = File(P($@"{R}\same.bin"), T0, (byte[])a.Clone());

        Assert.Equal(DiffStatus.SameTimeDifferentContent, Compare(l, r, WhenTimesEqual));
        Assert.Equal(DiffStatus.Identical, Compare(l, same, WhenTimesEqual));
    }

    [Fact]
    public void EmptyFiles_AreIdentical()
    {
        var l = File(P($@"{L}\e.txt"), T0, "");
        var r = File(P($@"{R}\e.txt"), T0, "");
        Assert.Equal(DiffStatus.Identical, Compare(l, r, WhenSizesEqual));
    }

    [Fact]
    public void ModeOff_NeverReadsContent()
    {
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0, "abc");
        var result = FileComparer.Compare(l, r, Off, new ThrowingComparer(new IOException("must not read")), CancellationToken.None);
        Assert.Equal(DiffStatus.Identical, result.Status);
    }

    [Fact]
    public void LockedFile_IsError()
    {
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0, "abc");
        _fs.GetFile(r.FullPath).AllowedFileShare = FileShare.None;

        var result = FileComparer.Compare(l, r, WhenTimesEqual, new ByteContentComparer(_fs), CancellationToken.None);

        Assert.Equal(DiffStatus.Error, result.Status);
        Assert.StartsWith("Content compare failed:", result.ErrorMessage);
    }

    [Theory]
    [InlineData(typeof(IOException))]
    [InlineData(typeof(UnauthorizedAccessException))]
    public void ReadErrors_AreError(Type exceptionType)
    {
        var l = File(P($@"{L}\a.txt"), T0, "abc");
        var r = File(P($@"{R}\a.txt"), T0, "abc");
        var ex = (Exception)Activator.CreateInstance(exceptionType, "Access is denied.")!;

        var result = FileComparer.Compare(l, r, WhenTimesEqual, new ThrowingComparer(ex), CancellationToken.None);

        Assert.Equal(DiffStatus.Error, result.Status);
        Assert.Equal("Content compare failed: Access is denied.", result.ErrorMessage);
    }

    [Fact]
    public void Cancellation_DuringContentCompare_Throws()
    {
        var data = new byte[ByteContentComparer.BufferSize * 4];
        var l = File(P($@"{L}\big.bin"), T0, data);
        var r = File(P($@"{R}\big.bin"), T0, (byte[])data.Clone());
        using var cts = new CancellationTokenSource();

        Assert.Throws<OperationCanceledException>(() =>
            FileComparer.Compare(l, r, WhenTimesEqual, new ByteContentComparer(_fs), cts.Token, _ => cts.Cancel()));
    }

    private sealed class ThrowingComparer(Exception ex) : IContentComparer
    {
        public bool AreEqual(string leftPath, string rightPath, Action<long>? bytesRead, CancellationToken ct) => throw ex;
    }
}
