using TimeDiff.Core.Comparison;
using TimeDiff.Core.Models;

namespace TimeDiff.Core.Tests;

public class TimestampComparerTests
{
    private static readonly DateTime T = new(2024, 1, 1, 0, 0, 0, DateTimeKind.Utc);
    private static readonly TimeSpan Tol = TimeSpan.FromSeconds(2);

    [Fact]
    public void Equal_IsIdentical() =>
        Assert.Equal(DiffStatus.Identical, TimestampComparer.Compare(T, T, Tol, false));

    [Fact]
    public void ExactlyAtTolerance_IsIdentical()
    {
        Assert.Equal(DiffStatus.Identical, TimestampComparer.Compare(T.AddSeconds(2), T, Tol, false));
        Assert.Equal(DiffStatus.Identical, TimestampComparer.Compare(T, T.AddSeconds(2), Tol, false));
    }

    [Fact]
    public void JustBelowTolerance_IsIdentical() =>
        Assert.Equal(DiffStatus.Identical, TimestampComparer.Compare(T.AddMilliseconds(1999), T, Tol, false));

    [Fact]
    public void JustAboveTolerance_IsDifferent()
    {
        Assert.Equal(DiffStatus.NewerLeft, TimestampComparer.Compare(T.AddMilliseconds(2001), T, Tol, false));
        Assert.Equal(DiffStatus.NewerRight, TimestampComparer.Compare(T, T.AddMilliseconds(2001), Tol, false));
    }

    [Fact]
    public void ZeroTolerance_DetectsAnyDifference() =>
        Assert.Equal(DiffStatus.NewerLeft, TimestampComparer.Compare(T.AddTicks(1), T, TimeSpan.Zero, false));

    [Theory]
    [InlineData(3600, false, DiffStatus.NewerLeft)]
    [InlineData(3600, true, DiffStatus.Identical)]
    [InlineData(-3600, true, DiffStatus.Identical)]
    [InlineData(3602, true, DiffStatus.Identical)]
    [InlineData(3598, true, DiffStatus.Identical)]
    [InlineData(3603, true, DiffStatus.NewerLeft)]
    [InlineData(-3603, true, DiffStatus.NewerRight)]
    [InlineData(1800, true, DiffStatus.NewerLeft)]
    [InlineData(7200, true, DiffStatus.NewerLeft)]
    public void DstHandling(int offsetSeconds, bool ignoreDst, DiffStatus expected) =>
        Assert.Equal(expected, TimestampComparer.Compare(T.AddSeconds(offsetSeconds), T, Tol, ignoreDst));
}
